use std::time::Duration;

use tracing::log::LevelFilter;
use v_exchanges_adapters::{
	Client,
	binance::{BinanceAuth, BinanceHttpUrl, BinanceOption, BinanceWebSocketUrl},
};

#[tokio::main]
async fn main() {
	env_logger::builder().filter_level(LevelFilter::Debug).init();
	let client = Client::default();

	//DO: methods for {price, klines, orderbook}
	//DO: receive interpreted
	//TODO: ping/pong every 3m (iirc some exchange likes it fast like that)

	let connection = client
		.websocket(
			"/ws/btcusdt@trade",
			|message| println!("{message}"), //
			[BinanceOption::WebSocketUrl(BinanceWebSocketUrl::Spot443)],
		)
		.await
		.expect("failed to connect websocket");

	//TODO: restructure so that we use it through loop-racing on .next().await
	//NOTE: the pings, pongs and whatever else are going to be handled higher up. Through the generics we only get eg Result<TradeEven, WsError>

	// receive messages
	tokio::time::sleep(Duration::from_secs(1)).await;

	// manually reconnect
	connection.reconnect_state().request_reconnect();

	// receive messages. we should see no missing message during reconnection
	tokio::time::sleep(Duration::from_secs(3)).await;

	// close the connection
	drop(connection);

	// wait for the "close" message to be logged
	tokio::time::sleep(Duration::from_secs(1)).await;

	dbg!(&"now private stuff");
	private_stuff().await;
}

async fn private_stuff() {
	let pubkey = std::env::var("BINANCE_TIGER_READ_PUBKEY").expect("no API pubkey found");
	let secret = std::env::var("BINANCE_TIGER_READ_SECRET").expect("no API secret found");
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
