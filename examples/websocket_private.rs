use std::{env, time::Duration};

use tracing::log::LevelFilter;
use v_exchanges_adapters::{
	Client,
	binance::{BinanceAuth, BinanceHttpUrl, BinanceOption, BinanceWebSocketUrl},
};

#[tokio::main]
async fn main() {
	env_logger::builder().filter_level(LevelFilter::Debug).init();
	let pubkey = env::var("BINANCE_TIGER_READ_PUBKEY").expect("no API pubkey found");
	let secret = env::var("BINANCE_TIGER_READ_SECRET").expect("no API secret found");
	let mut client = Client::default();
	client.update_default_option(BinanceOption::Pubkey(pubkey));
	client.update_default_option(BinanceOption::Secret(secret.into()));

	let key: serde_json::Value = client
		.post(
			"/sapi/v1/userDataStream/isolated",
			Some(&[("symbol", "BTCUSDT")]),
			[BinanceOption::HttpAuth(BinanceAuth::Key), BinanceOption::HttpUrl(BinanceHttpUrl::Spot)],
		)
		.await
		.expect("failed to get listen key");

	let _connection = client
		.websocket(
			&format!("/ws/{}", key["listenKey"].as_str().unwrap()),
			|message| println!("{}", message),
			[BinanceOption::WebSocketUrl(BinanceWebSocketUrl::Spot9443)],
		)
		.await
		.expect("failed to connect websocket");

	// receive messages
	tokio::time::sleep(Duration::from_secs(60)).await;
}
