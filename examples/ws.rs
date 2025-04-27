use std::str::FromStr as _;

use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let binance = Binance::default();
	let symbol = Symbol::from_str("BTCUSDT.P").unwrap();
	let mut rx = binance.ws_trades(symbol).await.unwrap();

	while let Some(trade_event) = rx.recv().await {
		println!("{trade_event:?}");
	}
}
