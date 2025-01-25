use v_exchanges::prelude::*;
use v_utils::prelude::*;

// Random test stuff, for dev purposes only
#[tokio::main]
async fn main() {
	clientside!();

	let market: AbsMarket = "Binance/Futures".into();
	let exchange = market.client();
	let klines = exchange.klines(("BTC", "USDT").into(), "1m".into(), 2.into(), market).await.unwrap();
	dbg!(klines);
}
