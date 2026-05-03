//! Records 60s of Binance BTCUSDT spot book activity to a temp catalog, then replays it back
//! and asserts ordering invariants.
//!
//! Run with `cargo r --example record_replay`.

use std::{collections::BTreeMap, str::FromStr as _, sync::Arc, time::Duration};

use tempfile::tempdir;
use v_exchanges::prelude::*;
use v_exchanges_adapters::binance::BinanceOption;
use v_exchanges_persistence::{Catalog, CatalogBookPersistor, Data, LiveClock, ReplayConfig, replay};
use v_utils::trades::Pair;

const RECORD_DURATION: Duration = Duration::from_secs(60);
const SNAPSHOT_FREQ: Duration = Duration::from_secs(30);

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let dir = tempdir().expect("tempdir");
	let catalog = Catalog::new(dir.path());

	let pair = Pair::from_str("BTCUSDT").unwrap();

	let mut binance = Binance::default();
	binance.update_default_option(BinanceOption::BookSnapshotFreq(Some(SNAPSHOT_FREQ)));

	let conn = binance.book_connection(&[pair], Instrument::Perp).await.expect("book_connection");
	let symbol_precisions: BTreeMap<_, _> = conn.symbol_precisions().clone();

	let persistor = CatalogBookPersistor::new(catalog.clone(), "binance", symbol_precisions, Arc::new(LiveClock));
	let mut conn = conn.with_persistor(Box::new(persistor));

	let start_ns = jiff::Timestamp::now().as_nanosecond() as i64;
	tracing::info!(start_ns, "recording {RECORD_DURATION:?} of Binance BTCUSDT spot book");

	let deadline = tokio::time::Instant::now() + RECORD_DURATION;
	let mut snapshots = 0_u32;
	let mut deltas = 0_u32;
	loop {
		tokio::select! {
			biased;
			_ = tokio::time::sleep_until(deadline) => break,
			res = conn.next() => match res.expect("ws stream errored") {
				BookUpdate::Snapshot(_) => snapshots += 1,
				BookUpdate::BatchDelta(_) => deltas += 1,
			}
		}
	}
	let end_ns = jiff::Timestamp::now().as_nanosecond() as i64;
	tracing::info!(snapshots, deltas, "stream closed; flushing + replaying");

	// Flush in-memory feathers to parquet before replay; rotation may not have triggered in 60s.
	conn.persistor_mut().expect("attached above").flush();
	drop(conn);

	let cfg = ReplayConfig::new(start_ns, end_ns);
	let out = replay(&catalog, "binance", &pair.to_string(), &cfg).expect("replay");
	tracing::info!(rows = out.len(), "replayed rows");

	assert!(snapshots >= 1, "expected at least one snapshot in 60s");
	assert!(deltas >= 1, "expected at least one delta in 60s");

	// Strictly monotonic non-decreasing ts_event across the merged stream.
	let mut prev: i64 = i64::MIN;
	for d in &out {
		let ts = d.ts_event();
		assert!(ts >= prev, "merged stream not monotonic: {prev} > {ts}");
		prev = ts;
	}

	// Best-effort delta gap check: between two consecutive snapshots, delta monotonic_seq is
	// strictly increasing (no requirement of contiguous sequence — emitter increments per level).
	let mut last_delta_seq: Option<u64> = None;
	for d in &out {
		match d {
			Data::Delta(row) => {
				if let Some(prev) = last_delta_seq {
					assert!(row.monotonic_seq > prev, "delta seq regression: {prev} -> {}", row.monotonic_seq);
				}
				last_delta_seq = Some(row.monotonic_seq);
			}
			Data::Snapshot(_) => last_delta_seq = None,
			_ => {}
		}
	}

	tracing::info!("record_replay: ok ({snapshots} snapshots, {deltas} deltas, {} replayed rows)", out.len());
}
