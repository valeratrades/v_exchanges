//! [`Data`] enum and Arrow schemas for the five storage lanes.
//!
//! All timestamps are UNIX nanoseconds in `i64`. Each row carries:
//! - `ts_event`: exchange-provided event time
//! - `ts_init`: ours, set on receive
//! - `monotonic_seq`: per-pair receive counter, gap-free between snapshots
//!
//! # Schema metadata
//!
//! Each Arrow schema attaches `{exchange, pair, price_precision, qty_precision, schema_version}`
//! via [`with_metadata`]. `schema_version` allows future format migrations.

use std::sync::Arc;

use arrow::{
	array::{Array, BinaryArray, BooleanArray, Int32Array, Int64Array, ListArray, RecordBatch, StringArray, UInt8Array, UInt32Array, UInt64Array},
	datatypes::{DataType, Field, Schema, SchemaRef},
};

use crate::catalog::Lane;

/// Format version stored in schema metadata. Bump on breaking schema changes.
pub const SCHEMA_VERSION: &str = "2";
/// UNIX nanoseconds since epoch.
pub type UnixNanos = i64;

/// One row per event in storage. Five fixed variants, no extensibility plumbing.
#[derive(Clone, Debug)]
pub enum Data {
	Snapshot(BookSnapshot),
	Delta(BookDelta),
	Trade(Trade),
	Close(Close),
	Custom(Custom),
}

impl Data {
	/// Initialization timestamp. Used for ordering and range pruning.
	pub fn ts_init(&self) -> UnixNanos {
		match self {
			Data::Snapshot(s) => s.ts_init,
			Data::Delta(d) => d.ts_init,
			Data::Trade(t) => t.ts_init,
			Data::Close(c) => c.ts_init,
			Data::Custom(c) => c.ts_init,
		}
	}

	pub fn ts_event(&self) -> UnixNanos {
		match self {
			Data::Snapshot(s) => s.ts_event,
			Data::Delta(d) => d.ts_event,
			Data::Trade(t) => t.ts_event,
			Data::Close(c) => c.ts_event,
			Data::Custom(c) => c.ts_event,
		}
	}

	pub fn monotonic_seq(&self) -> u64 {
		match self {
			Data::Snapshot(s) => s.monotonic_seq,
			Data::Delta(d) => d.monotonic_seq,
			Data::Trade(t) => t.monotonic_seq,
			Data::Close(_) | Data::Custom(_) => 0,
		}
	}
}

#[derive(Clone, Debug)]
pub struct BookSnapshot {
	pub ts_event: UnixNanos,
	pub ts_init: UnixNanos,
	pub monotonic_seq: u64,
	pub bid_prices: Vec<i32>,
	pub bid_qtys: Vec<u32>,
	pub ask_prices: Vec<i32>,
	pub ask_qtys: Vec<u32>,
}

#[derive(Clone, Copy, Debug)]
pub struct BookDelta {
	pub ts_event: UnixNanos,
	pub ts_init: UnixNanos,
	pub monotonic_seq: u64,
	/// `true` if the originating WS event broke the per-pair sequence chain (a wire-level drop
	/// was detected). Computed at WS-parse time; see exchange `Sequence` impls.
	pub gapped: bool,
	/// 0 = bid, 1 = ask.
	pub side: u8,
	pub price_raw: i32,
	/// `0` means delete this level.
	pub qty_raw: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Trade {
	pub ts_event: UnixNanos,
	pub ts_init: UnixNanos,
	pub monotonic_seq: u64,
	pub trade_id: u64,
	pub side: u8,
	pub price_raw: i32,
	pub qty_raw: u32,
}

#[derive(Clone, Debug)]
pub struct Close {
	pub ts_event: UnixNanos,
	pub ts_init: UnixNanos,
	pub reason: String,
}

#[derive(Clone, Debug)]
pub struct Custom {
	pub ts_event: UnixNanos,
	pub ts_init: UnixNanos,
	pub type_name: String,
	pub payload: Vec<u8>,
}

/// Per-file metadata embedded in the Arrow schema.
#[derive(Clone, Debug)]
pub struct FileMetadata {
	pub exchange: String,
	pub pair: String,
	pub price_precision: u8,
	pub qty_precision: u8,
}

impl FileMetadata {
	fn into_pairs(self) -> Vec<(String, String)> {
		vec![
			("exchange".into(), self.exchange),
			("pair".into(), self.pair),
			("price_precision".into(), self.price_precision.to_string()),
			("qty_precision".into(), self.qty_precision.to_string()),
			("schema_version".into(), SCHEMA_VERSION.into()),
		]
	}
}

// Decoders. Each takes a RecordBatch and yields the variant rows.
pub fn decode_snapshots(b: &RecordBatch) -> Vec<BookSnapshot> {
	let ts_event = col_i64(b, 0);
	let ts_init = col_i64(b, 1);
	let monotonic = col_u64(b, 2);
	let bid_prices = col_i32_list(b, 3);
	let bid_qtys = col_u32_list(b, 4);
	let ask_prices = col_i32_list(b, 5);
	let ask_qtys = col_u32_list(b, 6);

	(0..b.num_rows())
		.map(|i| BookSnapshot {
			ts_event: ts_event.value(i),
			ts_init: ts_init.value(i),
			monotonic_seq: monotonic.value(i),
			bid_prices: bid_prices[i].clone(),
			bid_qtys: bid_qtys[i].clone(),
			ask_prices: ask_prices[i].clone(),
			ask_qtys: ask_qtys[i].clone(),
		})
		.collect()
}
pub fn decode_deltas(b: &RecordBatch) -> Vec<BookDelta> {
	let ts_event = col_i64(b, 0);
	let ts_init = col_i64(b, 1);
	let monotonic = col_u64(b, 2);
	let gapped = col_bool(b, 3);
	let side = b.column(4).as_any().downcast_ref::<UInt8Array>().expect("u8 column");
	let price = b.column(5).as_any().downcast_ref::<Int32Array>().expect("i32 column");
	let qty = b.column(6).as_any().downcast_ref::<UInt32Array>().expect("u32 column");
	(0..b.num_rows())
		.map(|i| BookDelta {
			ts_event: ts_event.value(i),
			ts_init: ts_init.value(i),
			monotonic_seq: monotonic.value(i),
			gapped: gapped.value(i),
			side: side.value(i),
			price_raw: price.value(i),
			qty_raw: qty.value(i),
		})
		.collect()
}
pub fn decode_trades(b: &RecordBatch) -> Vec<Trade> {
	let ts_event = col_i64(b, 0);
	let ts_init = col_i64(b, 1);
	let monotonic = col_u64(b, 2);
	let trade_id = col_u64(b, 3);
	let side = b.column(4).as_any().downcast_ref::<UInt8Array>().expect("u8 column");
	let price = b.column(5).as_any().downcast_ref::<Int32Array>().expect("i32 column");
	let qty = b.column(6).as_any().downcast_ref::<UInt32Array>().expect("u32 column");
	(0..b.num_rows())
		.map(|i| Trade {
			ts_event: ts_event.value(i),
			ts_init: ts_init.value(i),
			monotonic_seq: monotonic.value(i),
			trade_id: trade_id.value(i),
			side: side.value(i),
			price_raw: price.value(i),
			qty_raw: qty.value(i),
		})
		.collect()
}
pub fn decode_closes(b: &RecordBatch) -> Vec<Close> {
	let ts_event = col_i64(b, 0);
	let ts_init = col_i64(b, 1);
	let reason = b.column(2).as_any().downcast_ref::<StringArray>().expect("string column");
	(0..b.num_rows())
		.map(|i| Close {
			ts_event: ts_event.value(i),
			ts_init: ts_init.value(i),
			reason: reason.value(i).to_owned(),
		})
		.collect()
}
pub fn decode_custom(b: &RecordBatch) -> Vec<Custom> {
	let ts_event = col_i64(b, 0);
	let ts_init = col_i64(b, 1);
	let type_name = b.column(2).as_any().downcast_ref::<StringArray>().expect("string column");
	let payload = b.column(3).as_any().downcast_ref::<BinaryArray>().expect("binary column");
	(0..b.num_rows())
		.map(|i| Custom {
			ts_event: ts_event.value(i),
			ts_init: ts_init.value(i),
			type_name: type_name.value(i).to_owned(),
			payload: payload.value(i).to_vec(),
		})
		.collect()
}

// Schema accessors used by the catalog and feather modules.
pub(crate) fn lane_schema(lane: Lane) -> SchemaRef {
	match lane {
		Lane::Snapshots => snapshots_schema(),
		Lane::Deltas => deltas_schema(),
		Lane::Trades => trades_schema(),
		Lane::Closes => closes_schema(),
		Lane::Custom => custom_schema(),
	}
}

fn deltas_schema() -> SchemaRef {
	Arc::new(Schema::new(vec![
		Field::new("ts_event", DataType::Int64, false),
		Field::new("ts_init", DataType::Int64, false),
		Field::new("monotonic_seq", DataType::UInt64, false),
		Field::new("gapped", DataType::Boolean, false),
		Field::new("side", DataType::UInt8, false),
		Field::new("price_raw", DataType::Int32, false),
		Field::new("qty_raw", DataType::UInt32, false),
	]))
}

fn snapshots_schema() -> SchemaRef {
	// The inner item is `nullable: true` to match the default produced by `ListBuilder`.
	let i32_list = || DataType::List(Arc::new(Field::new("item", DataType::Int32, true)));
	let u32_list = || DataType::List(Arc::new(Field::new("item", DataType::UInt32, true)));
	Arc::new(Schema::new(vec![
		Field::new("ts_event", DataType::Int64, false),
		Field::new("ts_init", DataType::Int64, false),
		Field::new("monotonic_seq", DataType::UInt64, false),
		Field::new("bid_prices", i32_list(), false),
		Field::new("bid_qtys", u32_list(), false),
		Field::new("ask_prices", i32_list(), false),
		Field::new("ask_qtys", u32_list(), false),
	]))
}

fn trades_schema() -> SchemaRef {
	Arc::new(Schema::new(vec![
		Field::new("ts_event", DataType::Int64, false),
		Field::new("ts_init", DataType::Int64, false),
		Field::new("monotonic_seq", DataType::UInt64, false),
		Field::new("trade_id", DataType::UInt64, false),
		Field::new("side", DataType::UInt8, false),
		Field::new("price_raw", DataType::Int32, false),
		Field::new("qty_raw", DataType::UInt32, false),
	]))
}

fn closes_schema() -> SchemaRef {
	Arc::new(Schema::new(vec![
		Field::new("ts_event", DataType::Int64, false),
		Field::new("ts_init", DataType::Int64, false),
		Field::new("reason", DataType::Utf8, false),
	]))
}

fn custom_schema() -> SchemaRef {
	Arc::new(Schema::new(vec![
		Field::new("ts_event", DataType::Int64, false),
		Field::new("ts_init", DataType::Int64, false),
		Field::new("type_name", DataType::Utf8, false),
		Field::new("payload", DataType::Binary, false),
	]))
}

pub(crate) fn with_metadata(schema: SchemaRef, meta: FileMetadata) -> SchemaRef {
	let mut s = (*schema).clone();
	s = s.with_metadata(meta.into_pairs().into_iter().collect());
	Arc::new(s)
}

// Internal helpers ---------------------------------------------------------

fn col_i64(b: &RecordBatch, idx: usize) -> &Int64Array {
	b.column(idx).as_any().downcast_ref::<Int64Array>().expect("i64 column")
}

fn col_u64(b: &RecordBatch, idx: usize) -> &UInt64Array {
	b.column(idx).as_any().downcast_ref::<UInt64Array>().expect("u64 column")
}

fn col_bool(b: &RecordBatch, idx: usize) -> &BooleanArray {
	b.column(idx).as_any().downcast_ref::<BooleanArray>().expect("bool column")
}

fn col_i32_list(b: &RecordBatch, idx: usize) -> Vec<Vec<i32>> {
	let list = b.column(idx).as_any().downcast_ref::<ListArray>().expect("list column");
	let values = list.values().as_any().downcast_ref::<Int32Array>().expect("i32 inner");
	let offsets = list.offsets();
	(0..list.len())
		.map(|i| {
			let start = offsets[i] as usize;
			let end = offsets[i + 1] as usize;
			(start..end).map(|j| values.value(j)).collect()
		})
		.collect()
}

fn col_u32_list(b: &RecordBatch, idx: usize) -> Vec<Vec<u32>> {
	let list = b.column(idx).as_any().downcast_ref::<ListArray>().expect("list column");
	let values = list.values().as_any().downcast_ref::<UInt32Array>().expect("u32 inner");
	let offsets = list.offsets();
	(0..list.len())
		.map(|i| {
			let start = offsets[i] as usize;
			let end = offsets[i + 1] as usize;
			(start..end).map(|j| values.value(j)).collect()
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use tempfile::tempdir;
	use v_exchanges_methods::{ExchangeName, Instrument, Symbol};

	use super::*;
	use crate::{
		catalog::{Catalog, LaneKey},
		feather::{Feather, RotationPolicy},
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

	fn round_trip<F: FnOnce(&mut Feather)>(lane: Lane, push: F) -> RecordBatch {
		let dir = tempdir().unwrap();
		let cat = Catalog::new(dir.path());
		let key = if lane == Lane::Custom {
			LaneKey::custom("test")
		} else {
			let symbol = Symbol::new("BTC-USDT".try_into().unwrap(), Instrument::Spot);
			LaneKey::book(lane, ExchangeName::Binance, symbol)
		};
		let mut f = match lane {
			Lane::Snapshots => Feather::new_snapshots(key, meta(), forever()),
			Lane::Deltas => Feather::new_deltas(key, meta(), forever()),
			Lane::Trades => Feather::new_trades(key, meta(), forever()),
			Lane::Closes => Feather::new_closes(key, meta(), forever()),
			Lane::Custom => Feather::new_custom(key, meta(), forever()),
		};
		push(&mut f);
		let path = f.flush(&cat).unwrap().expect("flush wrote a file");
		let batches = cat.read(&path).unwrap();
		assert_eq!(batches.len(), 1);
		batches.into_iter().next().unwrap()
	}

	#[test]
	fn snapshots_round_trip() {
		let batch = round_trip(Lane::Snapshots, |f| {
			f.push_snapshot(100, 110, 1, [100, 99].into_iter(), [10, 20].into_iter(), [101, 102].into_iter(), [5, 7].into_iter());
			f.push_snapshot(200, 210, 2, std::iter::empty(), std::iter::empty(), [103].into_iter(), [1].into_iter());
		});
		let decoded = decode_snapshots(&batch);
		assert_eq!(decoded.len(), 2);
		assert_eq!(decoded[0].bid_prices, vec![100, 99]);
		assert_eq!(decoded[1].ask_prices, vec![103]);
		assert_eq!(decoded[1].ts_event, 200);
	}

	#[test]
	fn deltas_round_trip() {
		let batch = round_trip(Lane::Deltas, |f| {
			f.push_delta(BookDelta {
				ts_event: 1,
				ts_init: 2,
				monotonic_seq: 9,
				gapped: true,
				side: 0,
				price_raw: 12345,
				qty_raw: 0,
			});
		});
		let decoded = decode_deltas(&batch);
		assert_eq!(decoded[0].price_raw, 12345);
		assert_eq!(decoded[0].qty_raw, 0);
		assert!(decoded[0].gapped);
	}

	#[test]
	fn trades_round_trip() {
		let batch = round_trip(Lane::Trades, |f| {
			f.push_trade(Trade {
				ts_event: 1,
				ts_init: 2,
				monotonic_seq: 3,
				trade_id: 4,
				side: 1,
				price_raw: 50,
				qty_raw: 60,
			});
		});
		let decoded = decode_trades(&batch);
		assert_eq!(decoded[0].trade_id, 4);
	}

	#[test]
	fn closes_round_trip() {
		let batch = round_trip(Lane::Closes, |f| {
			f.push_close(Close {
				ts_event: 1,
				ts_init: 2,
				reason: "halted".into(),
			});
		});
		let decoded = decode_closes(&batch);
		assert_eq!(decoded[0].reason, "halted");
	}

	#[test]
	fn custom_round_trip() {
		let batch = round_trip(Lane::Custom, |f| {
			f.push_custom(Custom {
				ts_event: 1,
				ts_init: 2,
				type_name: "alert".into(),
				payload: vec![0xde, 0xad, 0xbe, 0xef],
			});
		});
		let decoded = decode_custom(&batch);
		assert_eq!(decoded[0].payload, vec![0xde, 0xad, 0xbe, 0xef]);
	}
}
