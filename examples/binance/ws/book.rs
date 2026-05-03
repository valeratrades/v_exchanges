use std::{str::FromStr as _, time::Duration};

use v_exchanges::prelude::*;
use v_exchanges_adapters::binance::BinanceOption;
use v_utils::trades::Pair;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let pairs = vec![Pair::from_str("BTCUSDT").unwrap(), Pair::from_str("ETHUSDT").unwrap()];

	let mut binance = Binance::default();
	// Interleave REST book snapshots every 10 s (5 s per pair for two pairs).
	binance.update_default_option(BinanceOption::BookSnapshotFreq(Some(Duration::from_secs(10))));

	let mut spot = binance.ws_book(&pairs, Instrument::Spot).await.unwrap();
	let mut perp = binance.ws_book(&pairs, Instrument::Perp).await.unwrap();

	loop {
		tokio::select! {
			Ok(update) = spot.next() => {
				print_update("binance:spot", &update);
			}
			Ok(update) = perp.next() => {
				print_update("binance:perp", &update);
			}
		}
	}
}

fn print_update(source: &str, update: &BookUpdate) {
	let (kind, shape) = match update {
		BookUpdate::Snapshot(s) => ("SNAPSHOT", s),
		BookUpdate::BatchDelta(d) => ("DELTA", d),
	};
	let best_bid = shape.bids.iter().next_back().map(|(p, _)| Price {
		raw: *p,
		precision: shape.prec.price,
	});
	let best_ask = shape.asks.iter().next().map(|(p, _)| Price {
		raw: *p,
		precision: shape.prec.price,
	});
	println!(
		"[{source}] {kind:>8} | bids: {:>4} asks: {:>4} | best_bid: {:<12} best_ask: {:<12} | {}",
		shape.bids.len(),
		shape.asks.len(),
		best_bid.map_or("-".to_string(), |p| p.to_string()),
		best_ask.map_or("-".to_string(), |p| p.to_string()),
		shape.ts_event,
	);
}
