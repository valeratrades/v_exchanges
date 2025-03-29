#![feature(try_blocks)]
use std::{env, time::Duration};

use futures_util::{
	SinkExt as _, StreamExt as _,
	stream::{SplitSink, SplitStream},
};
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
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

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/*
trait: {
.handle_start() // will conditionally acquire a temp listen key
//DO: in: Value,  out: Vec<TungsteniteMessage>

.handle_message() // exchange-protocol level wrapper, will handle {auth, subscribe, etc} responses, before propagating contents to type interpreter `next`

-> Result<serde_json::Value>
Result<(serde_json::Value, Vec<TungsteniteMessage>), TungsteniteError>
}
*/

trait WsHandler {
	fn handle_start(&mut self) -> Vec<tungstenite::Message>; // will be a `Vec` for topic requests
	fn handle_message(&mut self, message: &serde_json::Value) -> Option<tungstenite::Message>;
}

struct BinanceWsHandler {}
impl WsHandler for BinanceWsHandler {
	fn handle_start(&mut self) -> Vec<tungstenite::Message> {
		vec![]
	}

	/// Determines if further communication is necessary. If the message received is the desired content, returns `None`.
	fn handle_message(&mut self, message: &serde_json::Value) -> Option<tungstenite::Message> {
		None //dbg: for now assume we never need to auth
	}
}

//Q: is it possible to get rid of Mutexes, if we make all methods take `&mut self`?
#[derive(Clone, Debug)]
struct WsConnection<H: WsHandler> {
	//connection_prerequisites: {url, handler: {auth, .handle_start()},
	//
	//Q: problem: authenticated streams require initial acquisition of a key.
	url: String,
	handler: Arc<Mutex<H>>,
	//Q: could maybe get rid of this Arc-Mutex
	inner: Arc<Mutex<Option<WsStream>>>,
}
impl<H: WsHandler> WsConnection<H> {
	pub fn new(url: String, handler: H) -> Self {
		let handler = Arc::new(Mutex::new(handler));
		let inner = Arc::new(Mutex::new(None));
		Self { url, handler, inner }
	}

	/// The main interface. All ws operations are hidden, only thing getting through are the content messages or the lack thereof.
	pub async fn next(&mut self) -> Result<String, tungstenite::Error> {
		let mut inner_lock = self.inner.lock().unwrap();
		if inner_lock.is_none() {
			let stream = self.connect().await.expect("TODO: .");
			*inner_lock = Some(stream);
		}
		let stream = inner_lock.as_mut().unwrap();

		//let (mut sink, mut stream) = inner_lock.expect("connect ensures this is some");

		while let Some(resp) = stream.next().await {
			let resp: Result<tungstenite::Message, tungstenite::Error> = resp; //dbg: lsp can't infer type
			match resp {
				Ok(succ_resp) => match succ_resp {
					tungstenite::Message::Text(text) => {
						//DO: assume serde_json::Value
						let value: serde_json::Value = serde_json::from_str(&text).expect("TODO: handle error");
						if let Some(further_communication) = self.handler.lock().unwrap().handle_message(&value) {
							stream.send(further_communication.clone()).await?; //HACK: probably can evade the clone()
							continue; // all the times we need to send something, it was not the desired message already.
						}

						//DO: interpret as target type
						return Ok(text.to_string());
					}
					tungstenite::Message::Binary(_) => {
						panic!("Received binary. But exchanges are not smart enough to send this, what is happening");
					}
					tungstenite::Message::Ping(_) => {
						stream.send(tungstenite::Message::Pong(Bytes::default())).await?;
						tracing::debug!("ponged");
						continue;
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
						*self.inner.lock().unwrap() = None;
						//TODO!!!!!: wait configured [Duration] before reconnect
						self.connect().await?;
						continue;
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

	async fn connect(&self) -> Result<WsStream, tungstenite::Error> {
		let (mut stream, http_resp) = tokio_tungstenite::connect_async(&self.url).await?;
		tracing::debug!("Ws handshake with server: {http_resp:?}");

		let messages = self.handler.lock().unwrap().handle_start();
		let mut message_stream = futures_util::stream::iter(messages).map(Ok);
		stream.send_all(&mut message_stream).await?;

		Ok(stream)
	}

	/// Returns on a message confirming the reconnection. All messages sent by the server before it accepting the first `Close` message are discarded.
	pub async fn request_reconnect(&self) -> Result<(), tungstenite::Error> {
		let mut lock = self.inner.lock().unwrap();
		if let Some(stream) = lock.as_mut() {
			stream.send(tungstenite::Message::Close(None)).await?;

			while let Some(resp) = stream.next().await {
				match resp {
					Ok(succ_resp) => match succ_resp {
						tungstenite::Message::Close(maybe_reason) => {
							tracing::debug!(?maybe_reason, "Server accepted close request");
							break;
						}
						_ => {
							// Ok to discard everything else, as this fn will only be triggered manually
							continue;
						}
					},
					Err(err) => {
						panic!("Error: {:?}", err);
					}
				}
			}
			*lock = None;
		}
		Ok(())
	}
}

#[tokio::main]
async fn main() {
	clientside!();
	dbg!("hardcoded impl for Binance");

	let url = "wss://stream.binance.com:443/ws/btcusdt@trade";
	//let url = "wss://stream.binance.com:443/ws/btcusiaednt@trade"; //binance error
	//let url = "wss://strbinance.com:443/ws/btcusiaednt@trade"; //connection error

	let handler = BinanceWsHandler {};
	let mut ws_connection = WsConnection::new(url.to_owned(), handler);

	let mut i = 0;
	while let Ok(trade_event) = ws_connection.next().await {
		println!("{trade_event:?}");
		i += 1;
		if i > 10 {
			break;
		}
	}
	println!("\ngonna request reeconnect\n");
	ws_connection.request_reconnect().await.unwrap();
	println!("\nran request reconnect\n");

	while let Ok(trade_event) = ws_connection.next().await {
		println!("{trade_event:?}");
		i += 1;
		if i > 20 {
			break;
		}
	}

	//DO: - deser into TradeEvent

	//Q: maybe wrap - need a way to encode Network Timeouts
	// So `next() -> TradeEvent' // Not a Result<TradeEvent, TungsteniteError>, because if they are not handled at ws level, we panic
}
