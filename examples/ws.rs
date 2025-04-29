use std::str::FromStr as _;

use v_exchanges::{core::ExchangeStream as _, prelude::*};
use v_utils::trades::Pair;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let binance = Binance::default();
	let pairs = vec![Pair::from_str("BTCUSDT").unwrap()];
	let instrument = Instrument::Perp;

	let mut trades_connection = binance.ws_trades(pairs, instrument).unwrap();
	while let Ok(trade_event) = trades_connection.next().await {
		dbg!(&trade_event);
	}
}
