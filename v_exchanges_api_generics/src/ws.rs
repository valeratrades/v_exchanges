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

use crate::AuthError;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// handle exchange-level events on the [WsConnection].
pub trait WsHandler {
	/// Returns a [WsConfig] that will be applied for all WebSocket connections handled by this handler.
	fn config(&self) -> WsConfig {
		WsConfig::default()
	}

	/// Called when the [WsConnection] is created and on reconnection. Returned messages will be sent back to the server as-is.
	///
	/// Handling of `listen-key`s or any other authentication methods exchange demands should be done here. Although oftentimes handling the auth will spread into the [handle_message](Self::handle_message) too.
	/// Can be ran multiple times (on every reconnect). Thus this inherently cannot be used to initiate connectionions based on a change of state (ie order creation).
	#[allow(unused_variables)]
	fn handle_auth(&mut self) -> Result<Vec<tungstenite::Message>, WsError> {
		Ok(vec![])
	}

	//Q: problem: can be either {String, serde_json::Value} //? other things?
	/*
	"position" 
	||
	json!{
  "id": "56374a46-3061-486b-a311-99ee972eb648",
  "method": "order.place",
  "params": {
    "symbol": "BTCUSDT",
    "side": "SELL",
    "type": "LIMIT",
    "timeInForce": "GTC",
    "price": "23416.10000000",
    "quantity": "0.00847000",
    "apiKey": "vmPUZE6mv9SD5VNHk4HlWFsOr6aKE2zvsw0MuIgwCIPy6utIco14y7Ju91duEh8A",
    "signature": "15af09e41c36f3cc61378c2fbe2c33719a03dd5eba8d0f9206fbda44de717c88",
    "timestamp": 1660801715431
	}
	}
	*/ 
	// handle_subcribe(&mut self, Vec<serde_json::Value>) -> Result<, WsError>

	/// Called when the [WsConnection] received a JSON-RPC value, returns messages to be sent to the server. If the message received is the desired content, should just return `None`.
	#[allow(unused_variables)]
	fn handle_jrpc(&mut self, jrpc: &serde_json::Value) -> Result<Option<Vec<tungstenite::Message>>, WsError> {
		Ok(None)
	}

	//A: use this iff spot&&perp binance accept listen-key refresh through stream 
	///// Additional POST communication with the exchange, not conditional on received messages, can be handled here.
	/////
	///// Really this is just for damn Binance with their stupid `listn-key` standard.
	//fn handle_post(&mut self) -> Result<Option<Vec<tungstenite::Message>>, WsError> {
	//	Ok(None)
	//}
}

#[derive(Debug)]
/// Main way to interact with the WebSocket APIs.
pub struct WsConnection<H: WsHandler> {
	url: Url,
	config: WsConfig,
	handler: H,
	stream: Option<WsConnectionStream>,
	last_reconnect_attempt: SystemTime, // not Tz-aware, as it will not escape the application boundary
}
#[derive(Debug, derive_more::Deref, derive_more::DerefMut)]
struct WsConnectionStream {
	#[deref_mut]
	#[deref]
	stream: WsStream,
	connected_since: SystemTime,
	last_unanswered_communication: Option<SystemTime>,
}
impl WsConnectionStream {
	fn new(stream: WsStream, connected_since: SystemTime) -> Self {
		Self {
			stream,
			connected_since,
			last_unanswered_communication: None,
		}
	}
}
impl<H: WsHandler> WsConnection<H> {
	#[allow(missing_docs)]
	pub fn new(url_suffix: &str, handler: H) -> Self {
		// expects here are not expected to be seen by the user. Correctness should theoretically be checked at the moment of merging provided options; before this is ever constructed.
		let config = handler.config();
		config.validate().expect("ws config is invalid");
		let url = match &config.base_url {
			Some(base_url) => base_url.join(url_suffix).expect("url is invalid"),
			None => Url::parse(url_suffix).expect("url is invalid"),
		};

		Self {
			url,
			config,
			handler,
			stream: None,
			last_reconnect_attempt: SystemTime::UNIX_EPOCH,
		}
	}

	/// The main interface. All ws operations are hidden, only thing getting through are the content messages or the lack thereof.
	//XXX: reconnections to parametrized streams don't work properly currently: rn we resend everything on reconnection, so it would open a new one. FIX: should instead persist a connection id.
	//Q: wtf do I do if we missed an updated there? Would you
	pub async fn next(&mut self) -> Result<serde_json::Value, WsError> {
		if let Some(inner) = &self.stream {
			if inner.connected_since + self.config.refresh_after < SystemTime::now() {
				tracing::info!("Refreshing connection, as `refresh_after` specified in WsConfig has elapsed ({:?})", self.config.refresh_after);
				self.reconnect().await?;
			}
		}
		if self.stream.is_none() {
			self.connect().await?;
		}
		//- at this point self.inner is Some

		// loop until we get actual content
		let json_rpc_value = loop {
			// force a response out of the server.
			let resp = {
				let timeout = match self.stream.as_ref().unwrap().last_unanswered_communication {
					Some(last_unanswered) => {
						let now = SystemTime::now();
						match last_unanswered + self.config.response_timeout > now {
							true => self.config.response_timeout,
							false => {
								tracing::error!(
									"Timeout for last unanswered communication ended before `.next()` was called. This likely indicates an implementation error on the clientside."
								);
								self.reconnect().await?;
								continue;
							}
						}
					}
					None => self.config.message_timeout,
				};

				let timeout_handle = tokio::time::timeout(timeout, {
					let stream = self.stream.as_mut().unwrap();
					stream.next()
				});
				match timeout_handle.await {
					Ok(Some(resp)) => {
						self.stream.as_mut().unwrap().last_unanswered_communication = None;
						resp
					}
					Ok(None) => {
						tracing::warn!("tungstenite couldn't read from the stream. Restarting.");
						self.reconnect().await?;
						continue;
					}
					Err(timeout_error) => {
						tracing::warn!("Message reception timed out after {:?} seconds. // {timeout_error}", timeout);
						{
							let stream = self.stream.as_mut().unwrap();
							match stream.last_unanswered_communication.is_some() {
								true => self.reconnect().await?,
								false => {
									// Reached standard message_timeout (one for messages sent when we're not forcing communication). So let's force it.
									self.send(tungstenite::Message::Ping(Bytes::default())).await?;
									continue;
								}
							}
						}
						continue;
					}
				}
			};

			// some response received, handle it
			match resp {
				Ok(succ_resp) => match succ_resp {
					tungstenite::Message::Text(text) => {
						let value: serde_json::Value =
							serde_json::from_str(&text).expect("API sent invalid JSON, which is completely unexpected. Disappointment is immeasurable and the day is ruined.");
						if let Some(further_communication) = { self.handler.handle_jrpc(&value)? } {
							self.send_all(further_communication).await?;
							continue; // only need to send responses when it's not yet the desired content.
						}
						tracing::trace!("{value:#?}"); // only log it after the `handle_message` has ran, as we're assuming that if it takes any actions, it will handle logging itself. (and that will likely be at a different level of important too)
						break value;
					}
					tungstenite::Message::Binary(_) => {
						panic!("Received binary. But exchanges are not smart enough to send this, what is happening");
					}
					tungstenite::Message::Ping(bytes) => {
						self.send(tungstenite::Message::Pong(bytes)).await?; // Binance specifically requires the exact ping's payload to be returned here: https://developers.binance.com/docs/binance-spot-api-docs/web-socket-streams
						tracing::debug!("ponged");
						continue;
					}
					// in most cases these are not seen, as it's sufficient to just answer to their [pings](tungstenite::Message::Ping). Our own pings are sent only when we haven't heard from the exchange for a while, in which case it's likely that it will not [pong](tungstenite::Message::Pong) back either.
					tungstenite::Message::Pong(_) => {
						tracing::info!("Received pong");
						continue;
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
						self.stream = None;
						self.reconnect().await?;
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
		if let Some(inner) = &mut self.stream {
			match messages.len() {
				0 => return Ok(()),
				1 => {
					inner.send(messages.into_iter().next().unwrap()).await?;
					inner.last_unanswered_communication = Some(SystemTime::now());
				}
				_ => {
					let mut message_stream = futures_util::stream::iter(messages).map(Ok);
					inner.send_all(&mut message_stream).await?;
					inner.last_unanswered_communication = Some(SystemTime::now());
				}
			};
			Ok(())
		} else {
			Err(tungstenite::Error::ConnectionClosed)
		}
	}

	async fn send(&mut self, message: tungstenite::Message) -> Result<(), tungstenite::Error> {
		self.send_all(vec![message]).await // vec cost is negligible
	}

	async fn connect(&mut self) -> Result<(), WsError> {
		tracing::info!("Connecting to {}...", self.url);
		{
			let now = SystemTime::now();
			let timeout = self.config.reconnect_cooldown;
			if self.last_reconnect_attempt + timeout > now {
				tracing::warn!("Reconnect cooldown is triggered. Likely indicative of a bad connection.");
				let duration = (self.last_reconnect_attempt + timeout).duration_since(now).unwrap();
				tokio::time::sleep(duration).await;
			}
		}
		self.last_reconnect_attempt = SystemTime::now();

		let (stream, http_resp) = tokio_tungstenite::connect_async(self.url.as_str()).await?;
		tracing::debug!("Ws handshake with server: {http_resp:#?}");

		let now = SystemTime::now();
		self.stream = Some(WsConnectionStream::new(stream, now));

		let messages = self.handler.handle_auth()?;
		Ok(self.send_all(messages).await?)
	}

	/// Sends the existing connection (if any) a `Close` message, and then simply drops it, opening a new one.
	///
	/// `pub` for testing only, does not {have to || is expected to} be exposed in any wrappers.
	pub async fn reconnect(&mut self) -> Result<(), WsError> {
		if self.stream.is_some() {
			tracing::info!("Dropping old connection before reconnecting...");
			{
				let stream = self.stream.as_mut().unwrap();
				stream.send(tungstenite::Message::Close(None)).await?;
				self.stream = None;
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
	/// Whether the connection should be authenticated. Normally implemented through a "listen key"
	pub auth: bool,
	/// Prefix which will be used for connections that started using this `WebSocketConfig`.
	///
	/// Ex: `"wss://example.com"`
	pub base_url: Option<Url>,
	/// Duration that should elapse between each attempt to start a new connection.
	///
	/// This matters because the [WebSocketConnection] reconnects on error. If the error
	/// continues to happen, it could spam the server if `connect_cooldown` is too short.
	pub reconnect_cooldown: Duration = Duration::from_secs(3),
	/// The [WebSocketConnection] will automatically reconnect when `refresh_after` has elapsed since the last connection started.
	pub refresh_after: Duration = Duration::from_hours(12),
	/// A reconnection will be triggered if no messages are received within this amount of time.
	pub message_timeout: Duration = Duration::from_mins(16), // assume all exchanges ping more frequently than this
	/// Timeout for the response to a message sent to the server.
	///
	/// Difference from the [message_timeout](Self::message_timeout) is that here we directly request communication. Eg: sending a Ping or attempting to auth.
	pub response_timeout: Duration = Duration::from_mins(2),
}
impl WsConfig {
	#[allow(missing_docs)]
	pub fn validate(&self) -> Result<()> {
		if self.reconnect_cooldown.is_zero() {
			bail!("connect_cooldown must be greater than 0");
		}
		if self.refresh_after.is_zero() {
			bail!("refresh_after must be greater than 0");
		}
		if self.message_timeout.is_zero() {
			bail!("message_timeout must be greater than 0");
		}
		if self.response_timeout.is_zero() {
			bail!("response_timeout must be greater than 0");
		}
		Ok(())
	}
}

#[derive(Debug, derive_more::Display, thiserror::Error, derive_more::From)]
pub enum WsError {
	Tungstenite(tungstenite::Error),
	Auth(AuthError),
	Other(eyre::Report),
}
