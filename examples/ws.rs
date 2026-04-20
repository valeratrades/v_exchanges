use std::str::FromStr as _;

use v_exchanges::prelude::*;
use v_utils::trades::Pair;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let binance = Binance::default();
	let pairs = vec![Pair::from_str("BTCUSDT").unwrap()];
	let instrument = Instrument::Perp;

	let handle = tokio::spawn(across_an_await_point(binance, pairs, instrument));
	handle.await;
}

async fn across_an_await_point(mut binance: Binance, pairs: Vec<Pair>, instrument: Instrument) {
	let mut trades_connection = binance.ws_trades(&pairs, instrument).await.unwrap();
	while let Ok(trade_event) = trades_connection.next().await {
		dbg!(&trade_event);
	}
	unreachable!();
}
