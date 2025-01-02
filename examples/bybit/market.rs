use std::env;

use v_exchanges::{bybit::Bybit, core::Exchange};
use v_exchanges_adapters::bybit::{BybitHttpAuth, BybitHttpUrl, BybitOption};

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));

	let mut bb = Bybit::default();

	let ticker: serde_json::Value =
		bb.0.get("/v5/market/tickers", &[("category", "spot"), ("symbol", "BTCUSDT")], [BybitOption::Default])
			.await
			.expect("failed to get ticker");
	println!("Ticker:\n{ticker}");

	if let (Ok(key), Ok(secret)) = (env::var("BYBIT_TIGER_READ_KEY"), env::var("BYBIT_TIGER_READ_SECRET")) {
		bb.0.update_default_option(BybitOption::Key(key));
		bb.0.update_default_option(BybitOption::Secret(secret));
		private(&mut bb).await;
	} else {
		eprintln!("BYBIT_TIGER_READ_KEY or BYBIT_TIGER_READ_SECRET is missing, skipping private API methods.");
	}

	//client.update_default_option(BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM));
	//let klines = client.futures_klines(("BTC", "USDT").into(), "1m".into(), 2, None, None).await.unwrap();
	//let price = client.futures_price(("BTC", "USDT").into()).await.unwrap();
	//dbg!(&klines, price);
	//
	//if let (Ok(key), Ok(secret)) = (env::var("BINANCE_TIGER_READ_KEY"), env::var("BINANCE_TIGER_READ_SECRET")) {
	//	client.update_default_option(BinanceOption::Key(key));
	//	client.update_default_option(BinanceOption::Secret(secret));
	//	let balance = client.futures_asset_balance("USDT".into()).await.unwrap();
	//	dbg!(&balance);
	//} else {
	//	eprintln!("BINANCE_TIGER_READ_KEY or BINANCE_TIGER_READ_SECRET is missing, skipping private API methods.");
	//}
}

async fn private(bb: &mut Bybit) {
	let balance: serde_json::Value = bb
		.get("/v5/account/wallet-balance", &[("accountType", "UNIFIED")], [BybitOption::HttpAuth(BybitHttpAuth::V3AndAbove)])
		.await
		.expect("failed to get balance");
	println!("Balance:\n{balance}");
}
