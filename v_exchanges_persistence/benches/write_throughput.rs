//! Instruction-count microbench for the [`Feather`] hot write path.
//!
//! Uses [`iai-callgrind`] (callgrind via valgrind) so results are deterministic across runs and
//! machines. Each function is run once per measurement.
//!
//! Run with `cargo bench -p v_exchanges_persistence`. Requires `valgrind` and the matching
//! `iai-callgrind-runner` binary on PATH (the flake's dev shell provides valgrind; the runner is
//! installed via `cargo install iai-callgrind-runner --version <matching>`).

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use tempfile::TempDir;
use v_exchanges_methods::{ExchangeName, Instrument, Symbol};
use v_exchanges_persistence::{
	Catalog, Feather, RotationPolicy,
	catalog::{Lane, LaneKey},
	schema::{BookDelta, FileMetadata},
};

const N_DELTAS: u64 = 100_000;
const N_SNAPSHOTS: u64 = 200;
const LEVELS: usize = 200;
fn meta() -> FileMetadata {
	FileMetadata {
		exchange: "binance".into(),
		pair: "BTC-USDT".into(),
		price_precision: 2,
		qty_precision: 5,
	}
}

fn test_symbol() -> Symbol {
	Symbol::new("BTC-USDT".try_into().unwrap(), Instrument::Spot)
}

fn fresh_deltas() -> (TempDir, Catalog, Feather) {
	let dir = tempfile::tempdir().unwrap();
	let cat = Catalog::new(dir.path());
	let key = LaneKey::book(Lane::Deltas, ExchangeName::Binance, test_symbol());
	let f = Feather::new_deltas(key, meta(), RotationPolicy { max_bytes: None, max_age: None });
	(dir, cat, f)
}

fn fresh_snapshots() -> (TempDir, Catalog, Feather) {
	let dir = tempfile::tempdir().unwrap();
	let cat = Catalog::new(dir.path());
	let key = LaneKey::book(Lane::Snapshots, ExchangeName::Binance, test_symbol());
	let f = Feather::new_snapshots(key, meta(), RotationPolicy { max_bytes: None, max_age: None });
	(dir, cat, f)
}

#[library_benchmark]
fn push_100k_deltas() {
	let (_dir, cat, mut f) = fresh_deltas();
	for i in 0..N_DELTAS {
		f.push_delta(BookDelta {
			ts_event: i as i64,
			ts_init: i as i64,
			monotonic_seq: i,
			gapped: false,
			side: (i & 1) as u8,
			price_raw: i as i32,
			qty_raw: i as u32,
		});
		f.maybe_flush(&cat).unwrap();
	}
	black_box(&f);
}

#[library_benchmark]
fn push_200_snapshots() {
	let bid_prices: Vec<i32> = (0..LEVELS as i32).collect();
	let bid_qtys: Vec<u32> = (0..LEVELS as u32).collect();
	let ask_prices: Vec<i32> = (LEVELS as i32..(LEVELS as i32 * 2)).collect();
	let ask_qtys: Vec<u32> = (LEVELS as u32..(LEVELS as u32 * 2)).collect();

	let (_dir, cat, mut f) = fresh_snapshots();
	for i in 0..N_SNAPSHOTS {
		f.push_snapshot(
			i as i64,
			i as i64,
			i,
			bid_prices.iter().copied(),
			bid_qtys.iter().copied(),
			ask_prices.iter().copied(),
			ask_qtys.iter().copied(),
		);
		f.maybe_flush(&cat).unwrap();
	}
	black_box(&f);
}

library_benchmark_group!(name = feather_push; benchmarks = push_100k_deltas, push_200_snapshots);
main!(library_benchmark_groups = feather_push);
