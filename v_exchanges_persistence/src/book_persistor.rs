//! [`BookPersistor`] implementation that captures every WS book event into
//! per-pair Feather buffers. Drives flushes inline on each event.
//!
//! ```ignore
//! let catalog = Catalog::new("/data");
//! let persistor = CatalogBookPersistor::new(catalog, "binance", pair_precisions, Arc::new(LiveClock));
//! let connection = exchange.ws_book(&pairs, instrument).await?.with_persistor(Box::new(persistor));
//! ```

use std::{collections::BTreeMap, sync::Arc};

use v_exchanges_methods::{BookPersistor, BookShape, ExchangeName, Instrument, PrecisionPriceQty, Symbol};
use v_utils::trades::Pair;

use crate::{
	catalog::{Catalog, Lane, LaneKey},
	clock::Clock,
	feather::{Feather, RotationPolicy},
	schema::{BookDelta, FileMetadata},
};

pub struct CatalogBookPersistor {
	catalog: Catalog,
	exchange: ExchangeName,
	clock: Arc<dyn Clock>,
	pairs: BTreeMap<Pair, PairLanes>,
}
impl CatalogBookPersistor {
	pub fn new(catalog: Catalog, exchange: ExchangeName, instrument: Instrument, pair_precisions: BTreeMap<Pair, PrecisionPriceQty>, clock: Arc<dyn Clock>) -> Self {
		let pairs = pair_precisions
			.into_iter()
			.map(|(pair, prec)| {
				let symbol = Symbol::new(pair, instrument);
				let meta = FileMetadata {
					exchange: exchange.to_string(),
					pair: pair.to_string(),
					price_precision: prec.price,
					qty_precision: prec.qty,
				};
				let lanes = PairLanes {
					monotonic: 0,
					snapshots: Feather::new_snapshots(LaneKey::book(Lane::Snapshots, exchange, symbol), meta.clone(), RotationPolicy::snapshots()),
					deltas: Feather::new_deltas(LaneKey::book(Lane::Deltas, exchange, symbol), meta, RotationPolicy::deltas()),
				};
				(pair, lanes)
			})
			.collect();
		Self { catalog, exchange, clock, pairs }
	}

	/// Flushes all in-memory buffers immediately. Useful at shutdown to avoid losing rows.
	pub fn flush_all(&mut self) -> Result<(), crate::catalog::CatalogError> {
		for lanes in self.pairs.values_mut() {
			lanes.snapshots.flush(&self.catalog)?;
			lanes.deltas.flush(&self.catalog)?;
		}
		Ok(())
	}
}

struct PairLanes {
	monotonic: u64,
	snapshots: Feather,
	deltas: Feather,
}

impl BookPersistor for CatalogBookPersistor {
	fn on_snapshot(&mut self, pair: Pair, shape: &BookShape) {
		let ts = shape.ts_event.as_nanosecond() as i64;
		let now = self.clock.now_ns();
		let catalog = &self.catalog;
		let lanes = self.pairs.get_mut(&pair).unwrap_or_else(|| panic!("pair {pair} not registered with persistor"));
		lanes.monotonic += 1;

		// BTreeMap iteration order is ascending price, matching the Arrow list semantics.
		lanes.snapshots.push_snapshot(
			ts,
			now,
			lanes.monotonic,
			shape.bids.keys().copied(),
			shape.bids.values().copied(),
			shape.asks.keys().copied(),
			shape.asks.values().copied(),
		);

		lanes.snapshots.maybe_flush(catalog).expect("snapshot feather flush failed: catalog state corrupted");
	}

	fn on_delta(&mut self, pair: Pair, shape: &BookShape, gapped: bool) {
		let ts = shape.ts_event.as_nanosecond() as i64;
		let now = self.clock.now_ns();
		let catalog = &self.catalog;
		let exchange = &self.exchange;
		let lanes = self
			.pairs
			.get_mut(&pair)
			.unwrap_or_else(|| panic!("pair {pair} not registered with persistor for exchange {exchange}"));

		// Emit one row per price level. Bids first (side=0), then asks (side=1). Every emitted
		// row of one event shares the same `gapped` value — it's a per-event property.
		for (&price, &qty) in &shape.bids {
			lanes.monotonic += 1;
			lanes.deltas.push_delta(BookDelta {
				ts_event: ts,
				ts_init: now,
				monotonic_seq: lanes.monotonic,
				gapped,
				side: 0,
				price_raw: price,
				qty_raw: qty,
			});
		}
		for (&price, &qty) in &shape.asks {
			lanes.monotonic += 1;
			lanes.deltas.push_delta(BookDelta {
				ts_event: ts,
				ts_init: now,
				monotonic_seq: lanes.monotonic,
				gapped,
				side: 1,
				price_raw: price,
				qty_raw: qty,
			});
		}

		lanes.deltas.maybe_flush(catalog).expect("delta feather flush failed: catalog state corrupted");
	}

	fn flush(&mut self) {
		self.flush_all().expect("flush_all failed: catalog state corrupted");
	}
}
