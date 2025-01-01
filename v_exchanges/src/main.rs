use std::env;

use v_exchanges_adapters::binance::{BinanceHttpUrl, BinanceOption};
use v_exchanges_deser::{binance::Client, core::Exchange};

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));

	let mut client = Client::new();

	//client.update_default_option(BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM));
	//let klines = client.futures_klines(("BTC", "USDT").into(), "1m".into(), None, None, None).await.unwrap();
	//let price = client.futures_price(("BTC", "USDT").into()).await.unwrap();
	//dbg!(&klines);

	let key = env::var("BINANCE_TIGER_READ_KEY").unwrap();
	let secret = env::var("BINANCE_TIGER_READ_SECRET").unwrap();
	client.update_default_option(BinanceOption::Key(key));
	client.update_default_option(BinanceOption::Secret(secret));
	let balance = client.futures_asset_balance("USDT".into()).await.unwrap();
	dbg!(&balance);
}
