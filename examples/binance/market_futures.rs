use std::env;

use v_exchanges::{
	binance,
	core::{Exchange, MarketTrait as _},
};

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));

	let m = binance::Market::Futures;
	let mut bn = m.client();

	let klines = bn.klines(("BTC", "USDT").into(), "1m".into(), 2.into(), m).await.unwrap();
	let price = bn.price(("BTC", "USDT").into(), m).await.unwrap();
	dbg!(&klines, price);

	if let (Ok(key), Ok(secret)) = (env::var("BINANCE_TIGER_READ_KEY"), env::var("BINANCE_TIGER_READ_SECRET")) {
		bn.auth(key, secret);
		let balance = bn.asset_balance("USDT".into(), m).await.unwrap();
		dbg!(&balance);
	} else {
		eprintln!("BINANCE_TIGER_READ_KEY or BINANCE_TIGER_READ_SECRET is missing, skipping private API methods.");
	}
}
