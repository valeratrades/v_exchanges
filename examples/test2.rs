#![feature(try_blocks)]
use std::{env, time::Duration};

use futures_util::{
	SinkExt as _, StreamExt as _,
	stream::{SplitSink, SplitStream},
};
use tokio::net::TcpStream;
use tokio_tungstenite::MaybeTlsStream;
use tracing::log::LevelFilter;
use tungstenite::{
	Bytes,
	client::IntoClientRequest as _,
	http::{Method, Request},
};
use v_exchanges::AbsMarket;
use v_exchanges_adapters::{
	Client,
	binance::{BinanceAuth, BinanceHttpUrl, BinanceOption, BinanceWebSocketUrl},
};
use v_utils::prelude::*;

type WebSocketSplitSink = SplitSink<tokio_tungstenite::WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>;

//Q: is it possible to get rid of Mutexes, if we make all methods take `&mut self`?
#[derive(Clone, Debug, derive_new::new)]
struct WsConnection {
	sink: Arc<Mutex<SplitSink<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, tungstenite::Message>>>,
	stream: Arc<Mutex<SplitStream<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>>>,
	state: Arc<Mutex<ReconnectState>>,
}
impl WsConnection {
	pub async fn next(&mut self) -> String {
		while let Some(resp) = self.stream.lock().unwrap().next().await {
			match resp {
				Ok(succ_resp) => match succ_resp {
					tungstenite::Message::Text(text) => {
						if self.state.lock().unwrap().is_reconnecting() {
							tracing::debug!("Ignoring a message received while reconnecting: {text}.");
							continue;
						}
						return text.to_string();
					}
					tungstenite::Message::Binary(bin) => {
						panic!("Received binary. But exchanges are not smart enough to send this, what is happening");
					}
					tungstenite::Message::Ping(_) => {
						self.send(tungstenite::Message::Pong(Bytes::default())).await;
						tracing::debug!("ponged");
					}
					//Q: Do I even need to send them? TODO: check if just replying to pings is sufficient
					tungstenite::Message::Pong(_) => {
						unimplemented!();
					}
					tungstenite::Message::Close(maybe_reason) => {
						match maybe_reason {
							Some(close_frame) => {
								//Q: maybe need to expose def of this for ind exchanges (so we can interpret the codes)
								tracing::info!("Server closed connection; reason: {close_frame:?}");
							}
							None => {
								tracing::info!("Server closed connection; no reason specified.");
							}
						}
						//TODO!!!!!: wait configured [Duration] before reconnect
						self.reconnect().await;
					}
					tungstenite::Message::Frame(_) => {
						unreachable!("Can't get from reading");
					}
				},
				Err(err) => {
					panic!("Error: {:?}", err);
				}
			}
		}
		todo!("Handle stream exhaustion (My guess is this can happen due to connection issues)"); //TODO: check when exactly `stream.next()` can fail
	}

	async fn send(&self, message: tungstenite::Message) {
		let r: Result<(), tungstenite::Error> = try {
			let mut sink = self.sink.lock().unwrap();
			sink.send(message).await?;
			sink.flush().await?;
		};
		r.expect("not sure it's handleable");
	}

	pub async fn request_reconnect(&self) {
		self.send(tungstenite::Message::Close(None)).await;
		//TODO!!!!!!!!!: poll until `Close` comes in
		//Q: wait, would I not rather open a new connection simultaneously?
	}

	/// At this point the connection is already closed.
	async fn reconnect(&self) {
		todo!();
	}
}

#[derive(Debug, Default)]
enum ReconnectState {
	#[default]
	None,
	Reconnecting,
}
impl ReconnectState {
	fn is_reconnecting(&self) -> bool {
		matches!(self, ReconnectState::Reconnecting)
	}
}

#[tokio::main]
async fn main() {
	clientside!();
	dbg!("hardcoded impl for Binance");

	let url = "wss://stream.binance.com:443/ws/btcusdt@trade";
	//let url = "wss://stream.binance.com:443/ws/btcusiaednt@trade";
	//let url = "wss://strbinance.com:443/ws/btcusiaednt@trade";

	let (websocket_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
	let (sink, stream) = websocket_stream.split();

	let mut ws_connection = WsConnection {
		sink: Arc::new(Mutex::new(sink)),
		stream: Arc::new(Mutex::new(stream)),
		state: Arc::new(Mutex::new(ReconnectState::None)),
	};
	while let trade_event = ws_connection.next().await {
		println!("{trade_event:?}");
	}

	//DO: - handle tungstenite errors (ie reconnect on all tungstenite errors but {Io, Url})

	//DO: - ping/pong handling, also close handling. On binary just panic for now.
	//DO: here we have a `String`
	//DO: - deser into TradeEvent

	//Q: maybe wrap - need a way to encode Network Timeouts
	// So `next() -> TradeEvent' // Not a Result<TradeEvent, TungsteniteError>, because if they are not handled at ws level, we panic
}
