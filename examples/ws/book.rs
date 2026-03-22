use std::str::FromStr as _;

use v_exchanges::prelude::*;
use v_utils::trades::Pair;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let pair = vec![Pair::from_str("BTCUSDT").unwrap()];

	let binance = Binance::default();
	let bybit = Bybit::default();

	let mut binance_spot = binance.ws_book(pair.clone(), Instrument::Spot).unwrap();
	let mut binance_perp = binance.ws_book(pair.clone(), Instrument::Perp).unwrap();
	let mut bybit_spot = bybit.ws_book(pair.clone(), Instrument::Spot).unwrap();
	let mut bybit_perp = bybit.ws_book(pair.clone(), Instrument::Perp).unwrap();

	loop {
		tokio::select! {
			Ok(update) = binance_spot.next() => {
				print_update("binance:spot", &update);
			}
			Ok(update) = binance_perp.next() => {
				print_update("binance:perp", &update);
			}
			Ok(update) = bybit_spot.next() => {
				print_update("bybit:spot", &update);
			}
			Ok(update) = bybit_perp.next() => {
				print_update("bybit:perp", &update);
			}
		}
	}
}

fn print_update(source: &str, update: &BookUpdate) {
	let (kind, shape) = match update {
		BookUpdate::Snapshot(s) => ("SNAPSHOT", s),
		BookUpdate::Delta(d) => ("DELTA", d),
	};
	let best_bid = shape.bids.first().map(|(p, _)| *p);
	let best_ask = shape.asks.first().map(|(p, _)| *p);
	println!(
		"[{source}] {kind:>8} | bids: {:>4} asks: {:>4} | best_bid: {:<12} best_ask: {:<12} | {}",
		shape.bids.len(),
		shape.asks.len(),
		best_bid.map_or("-".to_string(), |p| format!("{p:.2}")),
		best_ask.map_or("-".to_string(), |p| format!("{p:.2}")),
		shape.time,
	);
}
