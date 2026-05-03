//! Hot in-memory accumulators that flush to the [`Catalog`] on rotation triggers.
//!
//! One [`Feather`] per `(LaneKey, schema)`. Push events as they arrive; when the rotation policy
//! fires (size or wall-clock), the buffered batch is flushed as a Parquet file in the catalog.
//!
//! # Cancel-safety
//!
//! Push is sync and never blocks. Flush is async and runs on the caller's task. We do not spawn
//! background work — the owner is responsible for calling [`Feather::maybe_flush`] periodically
//! (e.g., from the same `next()` loop driving the WS stream).

use std::{
	path::PathBuf,
	sync::Arc,
	time::{Duration, Instant},
};

use arrow::{
	array::{ArrayRef, BinaryBuilder, BooleanBuilder, Int32Builder, Int64Builder, ListBuilder, RecordBatch, StringBuilder, UInt8Builder, UInt32Builder, UInt64Builder},
	datatypes::SchemaRef,
};

use crate::{
	catalog::{Catalog, CatalogError, Lane, LaneKey},
	schema::{BookDelta, Close, Custom, FileMetadata, Trade, UnixNanos, lane_schema, with_metadata},
};

/// Triggers for flushing the buffer. Either, both, or neither may fire.
#[derive(Clone, Copy, Debug)]
pub struct RotationPolicy {
	/// Approximate buffered size in bytes that triggers a flush.
	pub max_bytes: Option<usize>,
	/// Maximum age of the oldest row before flushing.
	pub max_age: Option<Duration>,
}

impl RotationPolicy {
	pub const fn snapshots() -> Self {
		Self {
			max_bytes: Some(64 * 1024 * 1024),
			max_age: Some(Duration::from_secs(6 * 3600)),
		}
	}

	pub const fn deltas() -> Self {
		Self {
			max_bytes: Some(256 * 1024 * 1024),
			max_age: Some(Duration::from_secs(3600)),
		}
	}

	pub const fn trades() -> Self {
		Self {
			max_bytes: Some(50 * 1024 * 1024),
			max_age: Some(Duration::from_secs(24 * 3600)),
		}
	}

	pub const fn closes() -> Self {
		Self {
			max_bytes: None,
			max_age: Some(Duration::from_secs(7 * 24 * 3600)),
		}
	}

	pub const fn custom() -> Self {
		Self::closes()
	}
}

pub struct Feather {
	key: LaneKey,
	schema: SchemaRef,
	policy: RotationPolicy,
	buffer: Buffer,
	rows: usize,
	approx_bytes: usize,
	/// `ts_init` of the oldest buffered row.
	oldest_ts: Option<UnixNanos>,
	/// `ts_init` of the newest buffered row.
	newest_ts: Option<UnixNanos>,
	/// Wall-clock instant at which the age-based flush trigger fires. `None` until the first
	/// push, or when [`RotationPolicy::max_age`] is `None`.
	age_deadline: Option<Instant>,
	/// Below this row count `maybe_flush` skips the size check entirely. Computed once from
	/// `policy.max_bytes` divided by a per-lane minimum row footprint.
	next_check_at_rows: usize,
}

impl Feather {
	pub fn new_snapshots(key: LaneKey, meta: FileMetadata, policy: RotationPolicy) -> Self {
		Self::new(key, Lane::Snapshots, meta, policy, Buffer::Snapshots(Box::default()))
	}

	pub fn new_deltas(key: LaneKey, meta: FileMetadata, policy: RotationPolicy) -> Self {
		Self::new(key, Lane::Deltas, meta, policy, Buffer::Deltas(Box::default()))
	}

	pub fn new_trades(key: LaneKey, meta: FileMetadata, policy: RotationPolicy) -> Self {
		Self::new(key, Lane::Trades, meta, policy, Buffer::Trades(Box::default()))
	}

	pub fn new_closes(key: LaneKey, meta: FileMetadata, policy: RotationPolicy) -> Self {
		Self::new(key, Lane::Closes, meta, policy, Buffer::Closes(Box::default()))
	}

	pub fn new_custom(key: LaneKey, meta: FileMetadata, policy: RotationPolicy) -> Self {
		Self::new(key, Lane::Custom, meta, policy, Buffer::Custom(Box::default()))
	}

	fn new(key: LaneKey, lane: Lane, meta: FileMetadata, policy: RotationPolicy, buffer: Buffer) -> Self {
		let schema = with_metadata(lane_schema(lane), meta);
		let next_check_at_rows = policy.max_bytes.map(|m| (m / per_row_min(lane)).max(64)).unwrap_or(usize::MAX);
		Self {
			key,
			schema,
			policy,
			buffer,
			rows: 0,
			approx_bytes: 0,
			oldest_ts: None,
			newest_ts: None,
			age_deadline: None,
			next_check_at_rows,
		}
	}

	pub fn key(&self) -> &LaneKey {
		&self.key
	}

	pub fn len(&self) -> usize {
		self.rows
	}

	pub fn is_empty(&self) -> bool {
		self.rows == 0
	}

	fn touch_ts(&mut self, ts: UnixNanos) {
		let was_empty = self.oldest_ts.is_none();
		self.oldest_ts = Some(self.oldest_ts.map_or(ts, |o| o.min(ts)));
		self.newest_ts = Some(self.newest_ts.map_or(ts, |n| n.max(ts)));
		if was_empty && let Some(age) = self.policy.max_age {
			self.age_deadline = Some(Instant::now() + age);
		}
	}

	pub fn push_snapshot(
		&mut self,
		ts_event: i64,
		ts_init: i64,
		monotonic_seq: u64,
		bid_prices: impl IntoIterator<Item = i32>,
		bid_qtys: impl IntoIterator<Item = u32>,
		ask_prices: impl IntoIterator<Item = i32>,
		ask_qtys: impl IntoIterator<Item = u32>,
	) {
		let Buffer::Snapshots(b) = &mut self.buffer else {
			panic!("wrong lane: expected snapshots");
		};
		b.ts_event.append_value(ts_event);
		b.ts_init.append_value(ts_init);
		b.monotonic_seq.append_value(monotonic_seq);
		let mut n_levels = 0usize;
		for (p, q) in bid_prices.into_iter().zip(bid_qtys) {
			b.bid_prices.values().append_value(p);
			b.bid_qtys.values().append_value(q);
			n_levels += 1;
		}
		b.bid_prices.append(true);
		b.bid_qtys.append(true);
		for (p, q) in ask_prices.into_iter().zip(ask_qtys) {
			b.ask_prices.values().append_value(p);
			b.ask_qtys.values().append_value(q);
			n_levels += 1;
		}
		b.ask_prices.append(true);
		b.ask_qtys.append(true);
		self.rows += 1;
		self.approx_bytes += 32 + 8 * n_levels;
		self.touch_ts(ts_init);
	}

	pub fn push_delta(&mut self, row: BookDelta) {
		let Buffer::Deltas(b) = &mut self.buffer else {
			panic!("wrong lane: expected deltas");
		};
		b.ts_event.append_value(row.ts_event);
		b.ts_init.append_value(row.ts_init);
		b.monotonic_seq.append_value(row.monotonic_seq);
		b.gapped.append_value(row.gapped);
		b.side.append_value(row.side);
		b.price_raw.append_value(row.price_raw);
		b.qty_raw.append_value(row.qty_raw);
		self.rows += 1;
		self.approx_bytes += 40;
		self.touch_ts(row.ts_init);
	}

	pub fn push_trade(&mut self, row: Trade) {
		let Buffer::Trades(b) = &mut self.buffer else {
			panic!("wrong lane: expected trades");
		};
		b.ts_event.append_value(row.ts_event);
		b.ts_init.append_value(row.ts_init);
		b.monotonic_seq.append_value(row.monotonic_seq);
		b.trade_id.append_value(row.trade_id);
		b.side.append_value(row.side);
		b.price_raw.append_value(row.price_raw);
		b.qty_raw.append_value(row.qty_raw);
		self.rows += 1;
		self.approx_bytes += 40;
		self.touch_ts(row.ts_init);
	}

	pub fn push_close(&mut self, row: Close) {
		let Buffer::Closes(b) = &mut self.buffer else {
			panic!("wrong lane: expected closes");
		};
		b.ts_event.append_value(row.ts_event);
		b.ts_init.append_value(row.ts_init);
		b.reason.append_value(&row.reason);
		self.rows += 1;
		self.approx_bytes += 16 + row.reason.len();
		self.touch_ts(row.ts_init);
	}

	pub fn push_custom(&mut self, row: Custom) {
		let Buffer::Custom(b) = &mut self.buffer else {
			panic!("wrong lane: expected custom");
		};
		b.ts_event.append_value(row.ts_event);
		b.ts_init.append_value(row.ts_init);
		b.type_name.append_value(&row.type_name);
		b.payload.append_value(&row.payload);
		self.rows += 1;
		self.approx_bytes += 16 + row.type_name.len() + row.payload.len();
		self.touch_ts(row.ts_init);
	}

	/// Returns true if the rotation policy fires.
	pub fn should_flush(&self) -> bool {
		if self.rows == 0 {
			return false;
		}
		if let Some(max) = self.policy.max_bytes
			&& self.approx_bytes >= max
		{
			return true;
		}
		self.age_deadline_passed()
	}

	fn age_deadline_passed(&self) -> bool {
		self.age_deadline.is_some_and(|t| Instant::now() >= t)
	}

	/// Encodes the buffered rows into a record batch and writes a parquet file via the catalog.
	/// Resets the buffer regardless of write outcome — the catalog is the source of truth, and
	/// failures here propagate so the caller can decide. Returns `Ok(None)` when buffer was empty.
	pub fn flush(&mut self, catalog: &Catalog) -> Result<Option<PathBuf>, CatalogError> {
		let Some((batch, ts_min, ts_max)) = self.build_batch() else {
			return Ok(None);
		};
		let path = catalog.write(&self.key, &batch, ts_min, ts_max)?;
		Ok(Some(path))
	}

	/// Convenience: flush only if [`Self::should_flush`] returns true. Hot-path optimized: skips
	/// the size check entirely until enough rows have accumulated to plausibly reach the byte
	/// threshold.
	pub fn maybe_flush(&mut self, catalog: &Catalog) -> Result<Option<PathBuf>, CatalogError> {
		if self.rows < self.next_check_at_rows && !self.age_deadline_passed() {
			return Ok(None);
		}
		if self.should_flush() { self.flush(catalog) } else { Ok(None) }
	}

	/// Finishes all builders into a `RecordBatch` and resets timestamp + size counters. Returns
	/// `None` when the buffer is empty.
	fn build_batch(&mut self) -> Option<(RecordBatch, UnixNanos, UnixNanos)> {
		if self.rows == 0 {
			return None;
		}
		let ts_min = self.oldest_ts.expect("set on first push");
		let ts_max = self.newest_ts.expect("set on first push");
		let arrays: Vec<ArrayRef> = match &mut self.buffer {
			Buffer::Snapshots(b) => b.finish(),
			Buffer::Deltas(b) => b.finish(),
			Buffer::Trades(b) => b.finish(),
			Buffer::Closes(b) => b.finish(),
			Buffer::Custom(b) => b.finish(),
		};
		let batch = RecordBatch::try_new(self.schema.clone(), arrays).expect("valid schema/array shape");
		self.rows = 0;
		self.approx_bytes = 0;
		self.oldest_ts = None;
		self.newest_ts = None;
		self.age_deadline = None;
		Some((batch, ts_min, ts_max))
	}
}

const fn per_row_min(lane: Lane) -> usize {
	match lane {
		Lane::Deltas | Lane::Trades => 40,
		Lane::Snapshots => 32,
		Lane::Closes | Lane::Custom => 16,
	}
}

/// Lane-typed Arrow builders. Each variant accumulates rows of the same shape directly into
/// `*Builder`s, eliminating an intermediate `Vec<struct>` allocation on the hot path.
enum Buffer {
	Snapshots(Box<SnapshotBuilders>),
	Deltas(Box<DeltaBuilders>),
	Trades(Box<TradeBuilders>),
	Closes(Box<CloseBuilders>),
	Custom(Box<CustomBuilders>),
}

struct DeltaBuilders {
	ts_event: Int64Builder,
	ts_init: Int64Builder,
	monotonic_seq: UInt64Builder,
	gapped: BooleanBuilder,
	side: UInt8Builder,
	price_raw: Int32Builder,
	qty_raw: UInt32Builder,
}
impl DeltaBuilders {
	fn finish(&mut self) -> Vec<ArrayRef> {
		vec![
			Arc::new(self.ts_event.finish()),
			Arc::new(self.ts_init.finish()),
			Arc::new(self.monotonic_seq.finish()),
			Arc::new(self.gapped.finish()),
			Arc::new(self.side.finish()),
			Arc::new(self.price_raw.finish()),
			Arc::new(self.qty_raw.finish()),
		]
	}
}

impl Default for DeltaBuilders {
	fn default() -> Self {
		Self {
			ts_event: Int64Builder::new(),
			ts_init: Int64Builder::new(),
			monotonic_seq: UInt64Builder::new(),
			gapped: BooleanBuilder::new(),
			side: UInt8Builder::new(),
			price_raw: Int32Builder::new(),
			qty_raw: UInt32Builder::new(),
		}
	}
}

struct SnapshotBuilders {
	ts_event: Int64Builder,
	ts_init: Int64Builder,
	monotonic_seq: UInt64Builder,
	bid_prices: ListBuilder<Int32Builder>,
	bid_qtys: ListBuilder<UInt32Builder>,
	ask_prices: ListBuilder<Int32Builder>,
	ask_qtys: ListBuilder<UInt32Builder>,
}
impl SnapshotBuilders {
	fn finish(&mut self) -> Vec<ArrayRef> {
		vec![
			Arc::new(self.ts_event.finish()),
			Arc::new(self.ts_init.finish()),
			Arc::new(self.monotonic_seq.finish()),
			Arc::new(self.bid_prices.finish()),
			Arc::new(self.bid_qtys.finish()),
			Arc::new(self.ask_prices.finish()),
			Arc::new(self.ask_qtys.finish()),
		]
	}
}

impl Default for SnapshotBuilders {
	fn default() -> Self {
		Self {
			ts_event: Int64Builder::new(),
			ts_init: Int64Builder::new(),
			monotonic_seq: UInt64Builder::new(),
			bid_prices: ListBuilder::new(Int32Builder::new()),
			bid_qtys: ListBuilder::new(UInt32Builder::new()),
			ask_prices: ListBuilder::new(Int32Builder::new()),
			ask_qtys: ListBuilder::new(UInt32Builder::new()),
		}
	}
}

struct TradeBuilders {
	ts_event: Int64Builder,
	ts_init: Int64Builder,
	monotonic_seq: UInt64Builder,
	trade_id: UInt64Builder,
	side: UInt8Builder,
	price_raw: Int32Builder,
	qty_raw: UInt32Builder,
}
impl TradeBuilders {
	fn finish(&mut self) -> Vec<ArrayRef> {
		vec![
			Arc::new(self.ts_event.finish()),
			Arc::new(self.ts_init.finish()),
			Arc::new(self.monotonic_seq.finish()),
			Arc::new(self.trade_id.finish()),
			Arc::new(self.side.finish()),
			Arc::new(self.price_raw.finish()),
			Arc::new(self.qty_raw.finish()),
		]
	}
}

impl Default for TradeBuilders {
	fn default() -> Self {
		Self {
			ts_event: Int64Builder::new(),
			ts_init: Int64Builder::new(),
			monotonic_seq: UInt64Builder::new(),
			trade_id: UInt64Builder::new(),
			side: UInt8Builder::new(),
			price_raw: Int32Builder::new(),
			qty_raw: UInt32Builder::new(),
		}
	}
}

struct CloseBuilders {
	ts_event: Int64Builder,
	ts_init: Int64Builder,
	reason: StringBuilder,
}
impl CloseBuilders {
	fn finish(&mut self) -> Vec<ArrayRef> {
		vec![Arc::new(self.ts_event.finish()), Arc::new(self.ts_init.finish()), Arc::new(self.reason.finish())]
	}
}

impl Default for CloseBuilders {
	fn default() -> Self {
		Self {
			ts_event: Int64Builder::new(),
			ts_init: Int64Builder::new(),
			reason: StringBuilder::new(),
		}
	}
}

struct CustomBuilders {
	ts_event: Int64Builder,
	ts_init: Int64Builder,
	type_name: StringBuilder,
	payload: BinaryBuilder,
}
impl CustomBuilders {
	fn finish(&mut self) -> Vec<ArrayRef> {
		vec![
			Arc::new(self.ts_event.finish()),
			Arc::new(self.ts_init.finish()),
			Arc::new(self.type_name.finish()),
			Arc::new(self.payload.finish()),
		]
	}
}

impl Default for CustomBuilders {
	fn default() -> Self {
		Self {
			ts_event: Int64Builder::new(),
			ts_init: Int64Builder::new(),
			type_name: StringBuilder::new(),
			payload: BinaryBuilder::new(),
		}
	}
}

#[cfg(test)]
mod tests {
	use tempfile::tempdir;
	use v_exchanges_methods::{ExchangeName, Instrument, Symbol};

	use super::*;

	fn meta() -> FileMetadata {
		FileMetadata {
			exchange: "binance".into(),
			pair: "BTC-USDT".into(),
			price_precision: 2,
			qty_precision: 5,
		}
	}

	#[test]
	fn flush_writes_parquet() {
		let dir = tempdir().unwrap();
		let catalog = Catalog::new(dir.path());
		let symbol = Symbol::new("BTC-USDT".try_into().unwrap(), Instrument::Spot);
		let key = LaneKey::book(Lane::Deltas, ExchangeName::Binance, symbol);
		let mut feather = Feather::new_deltas(key, meta(), RotationPolicy { max_bytes: Some(1), max_age: None });
		feather.push_delta(BookDelta {
			ts_event: 1,
			ts_init: 1,
			monotonic_seq: 1,
			gapped: false,
			side: 0,
			price_raw: 1,
			qty_raw: 1,
		});
		assert!(feather.should_flush());
		let path = feather.flush(&catalog).unwrap().unwrap();
		assert!(path.exists());
		assert_eq!(feather.len(), 0);
	}
}
