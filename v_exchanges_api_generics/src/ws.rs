use std::{
	time::{Duration, SystemTime},
	vec,
};

use futures_util::{SinkExt as _, StreamExt as _};
use tokio::net::TcpStream;
use eyre::{bail, Result};
use tokio_tungstenite::{
	MaybeTlsStream, WebSocketStream,
	tungstenite::{
		self, Bytes,
	},
};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// handle exchange-level events on the [WsConnection].
pub trait WsHandler {
	/// Returns a [WsConfig] that will be applied for all WebSocket connections handled by this handler.
	fn ws_config(&self) -> WsConfig;

	/// Called when a new connection has been started, and returns messages that should be sent to the server.
	///
	/// This could be called multiple times because the connection can be reconnected.
	fn handle_start(&mut self) -> Vec<tungstenite::Message> {
		vec![]
	}

	/// Called when the [WsConnection] received a message, returns messages to be sent to the server. If the message received is the desired content, should just return `None`.
	#[allow(unused_variables)]
	fn handle_message(&mut self, message: &serde_json::Value) -> Option<Vec<tungstenite::Message>> {
		None
	}
}

#[derive(Debug)]
/// Main way to interact with the WebSocket APIs.
pub struct WsConnection<H: WsHandler> {
	url: String,
	config: WsConfig,
	handler: H,
	inner: Option<WsStream>,
	last_reconnect_attempt: SystemTime, // will not escape application boundary, so no need to be Tz-aware
}
impl<H: WsHandler> WsConnection<H> {
	#[allow(missing_docs)]
	pub fn new(url: String, handler: H) -> Self {
		let config = handler.ws_config();
		config.validate().expect("ws config is invalid"); // not expected to be seen by the user. Correctness should theoretically be checked at the moment of merging provided options; before this is ever constructed.
		Self {
			url,
			config,
			handler,
			inner: None,
			last_reconnect_attempt: SystemTime::UNIX_EPOCH,
		}
	}

	/// The main interface. All ws operations are hidden, only thing getting through are the content messages or the lack thereof.
	pub async fn next(&mut self) -> Result<String, tungstenite::Error> {
		if self.inner.is_none() {
			self.connect().await?;
		}
		//- at this point self.inner is Some

		while let Some(resp) = { self.inner.as_mut().unwrap().next().await } {
			let resp: Result<tungstenite::Message, tungstenite::Error> = resp; //dbg: lsp can't infer type
			match resp {
				Ok(succ_resp) => match succ_resp {
					tungstenite::Message::Text(text) => {
						let value: serde_json::Value = serde_json::from_str(&text).expect("TODO: handle error");
						if let Some(further_communication) = { self.handler.handle_message(&value) } {
							//Q: check if it's actually more performant than default `send` (that effectively flushes on each)
							let mut messages_stream = futures_util::stream::iter(further_communication).map(Ok);
							{
								let stream = self.inner.as_mut().unwrap();
								stream.send_all(&mut messages_stream).await?; //HACK: probably can evade the clone()
							}
							continue; // only need to send responses when it's not yet the desired content.
						}

						//DO: interpret as target type
						return Ok(text.to_string());
					}
					tungstenite::Message::Binary(_) => {
						panic!("Received binary. But exchanges are not smart enough to send this, what is happening");
					}
					tungstenite::Message::Ping(_) => {
						{
							let stream = self.inner.as_mut().unwrap();
							stream.send(tungstenite::Message::Pong(Bytes::default())).await?;
						}
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
						self.inner = None;
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

	async fn connect(&mut self) -> Result<(), tungstenite::Error> {
		{
			let now = SystemTime::now();
			let timeout = self.config.connect_cooldown;
			if self.last_reconnect_attempt + timeout > now {
				tracing::debug!("Waiting for reconnect cooldown");
				let duration = (self.last_reconnect_attempt + timeout).duration_since(now).unwrap();
				tokio::time::sleep(duration).await;
			}
		}
		self.last_reconnect_attempt = SystemTime::now();

		let (mut stream, http_resp) = tokio_tungstenite::connect_async(&self.url).await?;
		tracing::debug!("Ws handshake with server: {http_resp:?}");

		let messages = self.handler.handle_start();
		let mut message_stream = futures_util::stream::iter(messages).map(Ok);
		stream.send_all(&mut message_stream).await?;

		self.inner = Some(stream);
		Ok(())
	}

	#[doc(hidden)]
	/// Returns on a message confirming the reconnection. All messages sent by the server before it accepting the first `Close` message are discarded.
	pub async fn request_reconnect(&mut self) -> Result<(), tungstenite::Error> {
		if self.inner.is_some() {
			{
				let stream = self.inner.as_mut().unwrap();
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
				self.inner = None;
			}
		}
		Ok(())
	}
}

/// Configuration for [WsHandler].
///
/// Should be returned by [WsHandler::ws_config()].
#[derive(Clone, Debug, Default)]
pub struct WsConfig {
	/// Prefix which will be used for connections that started using this `WebSocketConfig`.
	///
	/// Ex: `"wss://example.com"`
	pub url_prefix: String,
	/// Duration that should elapse between each attempt to start a new connection.
	///
	/// This matters because the [WebSocketConnection] reconnects on error. If the error
	/// continues to happen, it could spam the server if `connect_cooldown` is too short.
	pub connect_cooldown: Duration = Duration::from_millis(3000),
	/// The [WebSocketConnection] will automatically reconnect when `refresh_after` has elapsed since the last connection started.
	pub refresh_after: Duration = Duration::from_hours(12),
	/// A reconnection will be triggered if no messages are received within this amount of time.
	pub message_timeout: Duration = Duration::from_mins(16), // assume all exchanges ping more frequently than this
}
impl WsConfig {
	#[allow(missing_docs)]
	pub fn validate(&self) -> Result<()> {
		if self.connect_cooldown.is_zero() {
			bail!("connect_cooldown must be greater than 0");
		}
		if self.refresh_after.is_zero() {
			bail!("refresh_after must be greater than 0");
		}
		if self.message_timeout.is_zero() {
			bail!("message_timeout must be greater than 0");
		}
		Ok(())
	}
}
