use v_utils::prelude::*;
use v_exchanges::AbsMarket;

#[tokio::main]
async fn main() {
	clientside!();

	//TODO: switch to a generic exchange declaration, to show that this is available for all of them.
	//let binance = v_exchanges::binance::Binance::default();
	let m: AbsMarket = "Binance/Spot".into();
	let binance = m.client();
	let mut rx = binance.ws_trades(("BTC", "USDT").into(), m).await.unwrap();

	while let Some(trade_event) = rx.recv().await {
		println!("{trade_event:?}");
	}
}
