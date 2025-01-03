use std::env;

use v_exchanges::{binance::Binance, core::Exchange};
use v_exchanges_adapters::binance::{self, BinanceHttpUrl, BinanceOption};

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));

	let mut b = Binance::default();

	b.update_default_option(BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM));

	let klines = b.futures_klines(("BTC", "USDT").into(), "1m".into(), 2, None, None).await.unwrap();
	let price = b.futures_price(("BTC", "USDT").into()).await.unwrap();
	dbg!(&klines, price);

	if let (Ok(key), Ok(secret)) = (env::var("BINANCE_TIGER_READ_KEY"), env::var("BINANCE_TIGER_READ_SECRET")) {
		b.update_default_option(BinanceOption::Key(key));
		b.update_default_option(BinanceOption::Secret(secret));
		let balance = b.futures_asset_balance("USDT".into()).await.unwrap();
		dbg!(&balance);
	} else {
		eprintln!("BINANCE_TIGER_READ_KEY or BINANCE_TIGER_READ_SECRET is missing, skipping private API methods.");
	}
}
