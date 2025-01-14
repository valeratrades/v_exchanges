use std::env;

use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));

	let m: AbsMarket = "Binance/Futures".into();
	let mut c = m.client();

	println!("m: {m}");

	let exchange_info = c.exchange_info(m).await.unwrap();
	dbg!(&exchange_info.pairs.iter().take(2).collect::<Vec<_>>());

	let klines = c.klines(("BTC", "USDT").into(), "1m".into(), 2.into(), m).await.unwrap();
	let price = c.price(("BTC", "USDT").into(), m).await.unwrap();
	dbg!(&klines, price);

	if let (Ok(key), Ok(secret)) = (env::var("BINANCE_TIGER_READ_KEY"), env::var("BINANCE_TIGER_READ_SECRET")) {
		c.auth(key, secret);
		let balance = c.asset_balance("USDT".into(), m).await.unwrap();
		dbg!(&balance);
	} else {
		eprintln!("BINANCE_TIGER_READ_KEY or BINANCE_TIGER_READ_SECRET is missing, skipping private API methods.");
	}
}
