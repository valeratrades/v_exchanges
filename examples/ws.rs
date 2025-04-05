use v_exchanges::AbsMarket;
use v_utils::prelude::*;

#[tokio::main]
async fn main() {
	clientside!();

	let m: AbsMarket = "Binance/Spot".into();
	let binance = m.client();
	let mut rx = binance.ws_trades(("BTC", "USDT").into(), m).await.unwrap();

	while let Some(trade_event) = rx.recv().await {
		println!("{trade_event:?}");
	}
}
