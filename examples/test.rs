use std::time::Duration;

use tracing::log::LevelFilter;
use v_exchanges::prelude::*;
use v_exchanges_adapters::{
	Client,
	binance::{BinanceOption, BinanceWebSocketUrl},
};
use v_utils::prelude::*;

// Random test stuff, for dev purposes only

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
			|message| println!("{}", message),
			[BinanceOption::WebSocketUrl(BinanceWebSocketUrl::Spot443)],
		)
		.await
		.expect("failed to connect websocket");

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
}
