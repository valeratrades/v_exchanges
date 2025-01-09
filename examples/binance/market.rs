use std::env;

use v_exchanges::{binance::Binance, core::Exchange};
use v_exchanges_adapters::binance::{BinanceHttpUrl, BinanceOption};

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));

	let mut bn = Binance::default();

	bn.update_default_option(BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM));

	let klines = bn.futures_klines(("BTC", "USDT").into(), "1m".into(), 2.into()).await.unwrap();
	let price = bn.futures_price(("BTC", "USDT").into()).await.unwrap();
	dbg!(&klines, price);

	let spot_klines = bn.spot_klines(("BTC", "USDT").into(), "1m".into(), 2.into()).await.unwrap();
	dbg!(&spot_klines);

	if let (Ok(key), Ok(secret)) = (env::var("BINANCE_TIGER_READ_KEY"), env::var("BINANCE_TIGER_READ_SECRET")) {
		bn.auth(key, secret);
		let balance = bn.futures_asset_balance("USDT".into()).await.unwrap();
		dbg!(&balance);
	} else {
		eprintln!("BINANCE_TIGER_READ_KEY or BINANCE_TIGER_READ_SECRET is missing, skipping private API methods.");
	}
}
