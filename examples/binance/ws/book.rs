use std::str::FromStr as _;

use v_exchanges::prelude::*;
use v_utils::trades::Pair;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let pair = vec![Pair::from_str("BTCUSDT").unwrap()];

	let mut binance = Binance::default();

	let mut spot = binance.ws_book(&pair, Instrument::Spot).await.unwrap();
	let mut perp = binance.ws_book(&pair, Instrument::Perp).await.unwrap();

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
		BookUpdate::Delta(d) => ("DELTA", d),
	};
	let best_bid = shape.bids.first().map(|(p, _)| *p);
	let best_ask = shape.asks.first().map(|(p, _)| *p);
	println!(
		"[{source}] {kind:>8} | bids: {:>4} asks: {:>4} | best_bid: {:<12} best_ask: {:<12} | {}",
		shape.bids.len(),
		shape.asks.len(),
		best_bid.map_or("-".to_string(), |p| p.to_string()),
		best_ask.map_or("-".to_string(), |p| p.to_string()),
		shape.time,
	);
}
