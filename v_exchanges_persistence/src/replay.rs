//! Sequential replay over the catalog. K-way merges the five lanes by `(ts_event, monotonic_seq)`.
//!
//! # Anchor policy
//!
//! On `replay(start, end)`:
//! 1. Find the snapshot file with the latest `ts_max <= start` and the newest snapshot row in that
//!    file. If its `ts_event >= start - max_anchor_age`, the merged stream begins with that
//!    snapshot row to seed the book.
//! 2. Otherwise, [`tracing::warn!`] is emitted and replay starts from the earliest snapshot row
//!    `>= start`. Caller can choose to skip backtesting until warmup elapses.
//!
//! Strictly better than nautilus, which has no look-back logic.

use std::time::Duration;

use crate::{
	catalog::{Catalog, CatalogError, Lane, LaneKey},
	schema::{Data, UnixNanos, decode_closes, decode_deltas, decode_snapshots, decode_trades},
};

#[derive(Clone, Debug)]
pub struct ReplayConfig {
	pub start: UnixNanos,
	pub end: UnixNanos,
	pub max_anchor_age: Duration,
}

impl ReplayConfig {
	pub fn new(start: UnixNanos, end: UnixNanos) -> Self {
		Self {
			start,
			end,
			// 15 min default (plus exchange-rate slack the caller bakes in if needed).
			max_anchor_age: Duration::from_secs(15 * 60),
		}
	}
}

/// One pair only. Cross-pair ordering is the backtester's job.
pub fn replay(catalog: &Catalog, exchange: &str, pair: &str, config: &ReplayConfig) -> Result<Vec<Data>, CatalogError> {
	let book_lanes = [Lane::Snapshots, Lane::Deltas, Lane::Trades, Lane::Closes];
	let mut lanes: Vec<Vec<Data>> = Vec::new();

	let snapshots_key = LaneKey::book(Lane::Snapshots, exchange, pair);
	let anchor = pick_anchor(catalog, &snapshots_key, config)?;

	for lane in book_lanes {
		let key = LaneKey::book(lane, exchange, pair);
		let files = catalog.list_range(&key, config.start, config.end)?;
		let mut rows: Vec<Data> = Vec::new();
		for f in files {
			for batch in catalog.read(&f.path)? {
				match lane {
					Lane::Snapshots =>
						for r in decode_snapshots(&batch) {
							rows.push(Data::Snapshot(r));
						},
					Lane::Deltas =>
						for r in decode_deltas(&batch) {
							rows.push(Data::Delta(r));
						},
					Lane::Trades =>
						for r in decode_trades(&batch) {
							rows.push(Data::Trade(r));
						},
					Lane::Closes =>
						for r in decode_closes(&batch) {
							rows.push(Data::Close(r));
						},
					Lane::Custom => unreachable!(),
				}
			}
		}
		lanes.push(rows);
	}

	let mut merged: Vec<Data> = Vec::new();

	if let Some(anchor_row) = anchor {
		// Seed: emit the chosen snapshot row first, then drop any snapshot rows that fall before
		// the requested start so we don't double-emit the anchor or earlier history.
		let anchor_ts = anchor_row.ts_event();
		merged.push(anchor_row);
		// lane[0] is the snapshots lane; filter it.
		lanes[0].retain(|d| d.ts_event() > anchor_ts);
	}

	// Filter every lane to [start, end].
	for lane in &mut lanes {
		lane.retain(|d| d.ts_event() >= config.start && d.ts_event() <= config.end);
	}

	// K-way merge by (ts_event, monotonic_seq). Rows within a single lane are already sorted.
	let mut iters: Vec<std::iter::Peekable<std::vec::IntoIter<Data>>> = lanes.into_iter().map(|v| v.into_iter().peekable()).collect();

	loop {
		let mut best: Option<(usize, (UnixNanos, u64))> = None;
		for (i, it) in iters.iter_mut().enumerate() {
			let Some(peek) = it.peek() else { continue };
			let key = (peek.ts_event(), peek.monotonic_seq());
			match best {
				None => best = Some((i, key)),
				Some((_, cur)) =>
					if key < cur {
						best = Some((i, key));
					},
			}
		}
		match best {
			None => break,
			Some((i, _)) => merged.push(iters[i].next().expect("just peeked")),
		}
	}

	Ok(merged)
}

/// Returns the snapshot row to seed the book with, if a recent-enough one exists.
fn pick_anchor(catalog: &Catalog, snapshots_key: &LaneKey, config: &ReplayConfig) -> Result<Option<Data>, CatalogError> {
	// All snapshot files with ts_max <= start, latest first.
	let files = catalog.list(snapshots_key)?;
	let max_age_ns = config.max_anchor_age.as_nanos() as i64;

	// Find the latest snapshot file that ends at or before `start`. If none, also accept files
	// that span `start` (ts_min <= start <= ts_max) — their relevant rows are <= start.
	let candidate = files.iter().rev().find(|f| f.ts_min <= config.start);

	if let Some(file) = candidate {
		// Read all rows, take the newest `ts_event <= start`.
		let mut newest: Option<crate::schema::BookSnapshot> = None;
		for batch in catalog.read(&file.path)? {
			for row in decode_snapshots(&batch) {
				if row.ts_event > config.start {
					continue;
				}
				let take = match &newest {
					None => true,
					Some(cur) => row.ts_event > cur.ts_event,
				};
				if take {
					newest = Some(row);
				}
			}
		}
		if let Some(row) = newest
			&& config.start - row.ts_event <= max_age_ns
		{
			return Ok(Some(Data::Snapshot(row)));
		}
	}

	// No anchor within window: warn and let replay start from the first snapshot in range.
	let first_in_range = files.iter().find(|f| f.ts_max >= config.start);
	if let Some(f) = first_in_range {
		tracing::warn!(
			file = %f.path.display(),
			start = config.start,
			max_anchor_age_secs = config.max_anchor_age.as_secs(),
			"no recent anchor; replay will start from first snapshot >= start without book seed",
		);
	} else {
		tracing::warn!(start = config.start, "no snapshot files at or after start; replay will produce no anchor",);
	}
	Ok(None)
}

#[cfg(test)]
mod tests {
	use tempfile::tempdir;

	use super::*;
	use crate::{
		feather::{Feather, RotationPolicy},
		schema::{BookDelta, FileMetadata},
	};

	fn meta() -> FileMetadata {
		FileMetadata {
			exchange: "binance".into(),
			pair: "BTC-USDT".into(),
			price_precision: 2,
			qty_precision: 5,
		}
	}

	fn forever() -> RotationPolicy {
		RotationPolicy { max_bytes: None, max_age: None }
	}

	/// Drives a fresh `Feather` to write a single-snapshot parquet file at exact `ts_event`.
	fn write_snapshot(cat: &Catalog, key: LaneKey, ts: i64) {
		let mut f = Feather::new_snapshots(key, meta(), forever());
		f.push_snapshot(ts, ts, ts as u64, ts as u64, [100].into_iter(), [10].into_iter(), [101].into_iter(), [10].into_iter());
		f.flush(cat).unwrap();
	}

	fn write_delta(cat: &Catalog, key: LaneKey, row: BookDelta) {
		let mut f = Feather::new_deltas(key, meta(), forever());
		f.push_delta(row);
		f.flush(cat).unwrap();
	}

	fn delta(ts: i64, mseq: u64) -> BookDelta {
		BookDelta {
			ts_event: ts,
			ts_init: ts,
			sequence: mseq,
			monotonic_seq: mseq,
			side: 0,
			price_raw: 0,
			qty_raw: 0,
		}
	}

	#[test]
	fn anchor_within_window_seeds_book() {
		let dir = tempdir().unwrap();
		let cat = Catalog::new(dir.path());
		let snaps_key = || LaneKey::book(Lane::Snapshots, "binance", "BTC-USDT");
		let deltas_key = || LaneKey::book(Lane::Deltas, "binance", "BTC-USDT");

		// 1 ns = 1 unit. Use seconds for clarity: 1s = 1_000_000_000.
		let s = 1_000_000_000_i64;
		write_snapshot(&cat, snaps_key(), 0); // way old
		write_snapshot(&cat, snaps_key(), 50 * s); // 50s before start (anchor)
		write_snapshot(&cat, snaps_key(), 120 * s); // inside range

		write_delta(&cat, deltas_key(), delta(110 * s, 1));

		let cfg = ReplayConfig::new(100 * s, 200 * s); // window 15 min default
		let out = replay(&cat, "binance", "BTC-USDT", &cfg).unwrap();
		// First row must be the anchor snapshot at 50s (within 15min).
		assert!(matches!(&out[0], Data::Snapshot(s) if s.ts_event == 50 * 1_000_000_000));
		// Second row should be the delta at 110s.
		assert!(matches!(&out[1], Data::Delta(d) if d.ts_event == 110 * 1_000_000_000));
		// Third row should be the snapshot at 120s.
		assert!(matches!(&out[2], Data::Snapshot(s) if s.ts_event == 120 * 1_000_000_000));
	}

	#[test]
	fn anchor_out_of_window_skipped() {
		let dir = tempdir().unwrap();
		let cat = Catalog::new(dir.path());
		let snaps_key = || LaneKey::book(Lane::Snapshots, "binance", "BTC-USDT");
		let s = 1_000_000_000_i64;
		// Anchor 1 hour before start — outside the default 15-min window.
		write_snapshot(&cat, snaps_key(), 0);
		write_snapshot(&cat, snaps_key(), 120 * s);

		let cfg = ReplayConfig::new(60 * 60 * s, 2 * 60 * 60 * s);
		let out = replay(&cat, "binance", "BTC-USDT", &cfg).unwrap();
		// No anchor should be prepended; first row is the in-range snapshot at 120s.
		// (which falls before start, so it's filtered too — out should be empty)
		assert!(out.is_empty(), "got {} rows", out.len());
	}
}
