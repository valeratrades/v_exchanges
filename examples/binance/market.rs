use std::env;

use v_exchanges::{binance::Client, core::Exchange};
use v_exchanges_adapters::binance::{self, BinanceHttpUrl, BinanceOption};

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));

	let mut client = Client::new();

	client.update_default_option(BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM));

	//before implementing the trait for bybit too, was able to just do eg: `let klines = client.futures_klines(("BTC", "USDT").into(), "1m".into(), 2, None, None).await.unwrap();`

	let klines = <Client as Exchange<binance::BinanceOptions>>::futures_klines(&client, ("BTC", "USDT").into(), "1m".into(), 2, None, None)
		.await
		.unwrap();
	let price = <Client as Exchange<binance::BinanceOptions>>::futures_price(&client, ("BTC", "USDT").into()).await.unwrap();
	dbg!(&klines, price);

	if let (Ok(key), Ok(secret)) = (env::var("BINANCE_TIGER_READ_KEY"), env::var("BINANCE_TIGER_READ_SECRET")) {
		client.update_default_option(BinanceOption::Key(key));
		client.update_default_option(BinanceOption::Secret(secret));
		let balance = <Client as Exchange<binance::BinanceOptions>>::futures_asset_balance(&client, "USDT".into()).await.unwrap();
		dbg!(&balance);
	} else {
		eprintln!("BINANCE_TIGER_READ_KEY or BINANCE_TIGER_READ_SECRET is missing, skipping private API methods.");
	}
}
