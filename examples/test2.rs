use std::{env, time::Duration};

use futures_util::stream::StreamExt as _;
use tracing::log::LevelFilter;
use tungstenite::{
	client::IntoClientRequest as _,
	http::{Method, Request},
};
use v_exchanges::AbsMarket;
use v_exchanges_adapters::{
	Client,
	binance::{BinanceAuth, BinanceHttpUrl, BinanceOption, BinanceWebSocketUrl},
};
use v_utils::prelude::*;

#[tokio::main]
async fn main() {
	clientside!();
	dbg!("hardcoded impl for Binance");

	let url = "wss://stream.binance.com:443/ws/btcusdt@trade";
	//let mut request = url.into_client_request().unwrap();
	//request.headers_mut().insert("api-key", "42".parse().unwrap());

	let (websocket_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
	let (mut sink, mut stream) = websocket_stream.split();

	while let Some(message) = stream.next().await {
		dbg!(&message);
	}

	//let messages = connection.handler.lock().handle_start();
	//for message in messages {
	//	sink.send(message.into_message()).await?;
	//}
	//sink.flush().await?;
	//
	//// fetch_not is unstable so we use fetch_xor
	//let id = connection.next_connection_id.fetch_xor(true, Ordering::SeqCst);
	//
	//// Create the future that will process the stream
	//let process_future = async move {
	//	let connection = connection.clone();
	//
	//	while let Some(message) = stream.next().await {
	//		// send the received message to the task running feed_handler
	//		if connection.message_tx.send((id, FeederMessage::Message(message))).is_err() {
	//			// the channel is closed. we can't disconnect because we don't have the sink
	//			tracing::debug!("Ws message receiver is closed; abandon connection");
	//			return;
	//		}
	//	}
	//	// the underlying Ws connection was closed
	//
	//	drop(connection.message_tx.send((id, FeederMessage::ConnectionClosed))); // this may be Err
	//	tracing::info!("Ws stream closed");
	//};
}
