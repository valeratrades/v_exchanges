use std::{
	time::{Duration, SystemTime},
	vec,
};

use eyre::{Result, bail};
use futures_util::{SinkExt as _, StreamExt as _};
use reqwest::Url;
use tokio::net::TcpStream;
use tokio_tungstenite::{
	MaybeTlsStream, WebSocketStream,
	tungstenite::{self, Bytes},
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
	url: Url,
	config: WsConfig,
	handler: H,
	inner: Option<WsConnectionInner>,
	last_reconnect_attempt: SystemTime, // not Tz-aware, as it will not escape the application boundary
}
#[derive(Debug, derive_more::Deref, derive_more::DerefMut)]
struct WsConnectionInner {
	#[deref_mut]
	#[deref]
	stream: WsStream,
	connected_since: SystemTime,
}
impl<H: WsHandler> WsConnection<H> {
	#[allow(missing_docs)]
	pub fn new(url: &str, handler: H) -> Self {
		// expects here are not expected to be seen by the user. Correctness should theoretically be checked at the moment of merging provided options; before this is ever constructed.
		let config = handler.ws_config();
		config.validate().expect("ws config is invalid");
		let url = match &config.base_url {
			Some(base_url) => base_url.join(url).expect("url is invalid"),
			None => Url::parse(url).expect("url is invalid"),
		};

		Self {
			url,
			config,
			handler,
			inner: None,
			last_reconnect_attempt: SystemTime::UNIX_EPOCH,
		}
	}

	/// The main interface. All ws operations are hidden, only thing getting through are the content messages or the lack thereof.
	pub async fn next(&mut self) -> Result<serde_json::Value, tungstenite::Error> {
		if let Some(inner) = &self.inner {
			if inner.connected_since + self.config.refresh_after < SystemTime::now() {
				tracing::info!("Refreshing connection, as `refresh_after` specified in WsConfig has elapsed ({:?})", self.config.refresh_after);
				self.reconnect().await?;
			}
		}
		if self.inner.is_none() {
			self.connect().await?;
		}
		//- at this point self.inner is Some

		let json_rpc_value = loop {
			// force a response out of the server.
			let timeout_handle = tokio::time::timeout(self.config.message_timeout, {
				let stream = self.inner.as_mut().unwrap();
				stream.next()
			});
			let resp = match timeout_handle.await {
				Ok(Some(resp)) => resp,
				Ok(None) => {
					tracing::warn!("tungstenite couldn't read from the stream. Restarting.");
					self.reconnect().await?;
					continue;
				}
				Err(timeout_error) => {
					tracing::warn!("Message reception timed out after {:?} seconds. // {timeout_error}", self.config.message_timeout);
					self.reconnect().await?;
					continue;
				}
			};

			// some response received, handle it
			match resp {
				Ok(succ_resp) => match succ_resp {
					tungstenite::Message::Text(text) => {
						let value: serde_json::Value =
							serde_json::from_str(&text).expect("API sent invalid JSON, which is completely unexpected. Disappointment is immeasurable and the day is ruined.");
						if let Some(further_communication) = { self.handler.handle_message(&value) } {
							self.send_all(further_communication).await?;
							continue; // only need to send responses when it's not yet the desired content.
						}
						break value;
					}
					tungstenite::Message::Binary(_) => {
						panic!("Received binary. But exchanges are not smart enough to send this, what is happening");
					}
					tungstenite::Message::Ping(_) => {
						self.send(tungstenite::Message::Pong(Bytes::default())).await?;
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
					//TODO!!!!!!: match on error types, attempt reconnect if that could help it
					panic!("Error: {:?}", err);
				}
			}
		};
		Ok(json_rpc_value)
	}

	async fn send_all(&mut self, messages: Vec<tungstenite::Message>) -> Result<(), tungstenite::Error> {
		if let Some(inner) = &mut self.inner {
			let r = match messages.len() {
				0 => return Ok(()),
				1 => inner.send(messages.into_iter().next().unwrap()).await?,
				_ => {
					let mut message_stream = futures_util::stream::iter(messages).map(Ok);
					inner.send_all(&mut message_stream).await?
				}
			};
			//TODO!!!!!!: update last_communication time on .inner, which starts a faster timeout for receiving next message. (it being faster is not guaranteed, so just .min() the two configured)
			Ok(())
		} else {
			Err(tungstenite::Error::ConnectionClosed)
		}
	}

	async fn send(&mut self, message: tungstenite::Message) -> Result<(), tungstenite::Error> {
		self.send_all(vec![message]).await // vec cost is negligible
	}

	async fn connect(&mut self) -> Result<(), tungstenite::Error> {
		tracing::info!("Connecting to {}...", self.url);
		{
			let now = SystemTime::now();
			let timeout = self.config.connect_cooldown;
			if self.last_reconnect_attempt + timeout > now {
				tracing::warn!("Reconnect cooldown is triggered. Likely indicative of a bad connection.");
				let duration = (self.last_reconnect_attempt + timeout).duration_since(now).unwrap();
				tokio::time::sleep(duration).await;
			}
		}
		self.last_reconnect_attempt = SystemTime::now();

		let (stream, http_resp) = tokio_tungstenite::connect_async(self.url.as_str()).await?;
		tracing::debug!("Ws handshake with server: {http_resp:?}");

		let now = SystemTime::now();
		self.inner = Some(WsConnectionInner { stream, connected_since: now });

		let messages = self.handler.handle_start();
		self.send_all(messages).await
	}

	/// Sends the existing connection (if any) a `Close` message, and then simply drops it, opening a new one.
	pub async fn reconnect(&mut self) -> Result<(), tungstenite::Error> {
		tracing::info!("Dropping old connection before reconnecting...");
		if self.inner.is_some() {
			{
				let stream = self.inner.as_mut().unwrap();
				stream.send(tungstenite::Message::Close(None)).await?;
				self.inner = None;
			}
		}
		self.connect().await
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
	pub base_url: Option<Url>,
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
