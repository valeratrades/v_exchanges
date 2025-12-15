use std::{
	collections::HashSet,
	time::{Duration, SystemTime},
	vec,
};

use eyre::{Result, bail};
use futures_util::{SinkExt as _, StreamExt as _};
use jiff::Timestamp;
use reqwest::Url;
use tokio::net::TcpStream;
use tokio_tungstenite::{
	MaybeTlsStream, WebSocketStream,
	tungstenite::{self, Bytes},
};
use tracing::instrument;

use crate::{AuthError, UrlError};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// handle exchange-level events on the [WsConnection].
pub trait WsHandler: std::fmt::Debug {
	/// Returns a [WsConfig] that will be applied for all WebSocket connections handled by this handler.
	fn config(&self) -> Result<WsConfig, UrlError> {
		Ok(WsConfig::default())
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
	  - and then the latter could be requiring signing
	  */
	#[allow(unused_variables)]
	fn handle_subscribe(&mut self, topics: HashSet<Topic>) -> Result<Vec<tungstenite::Message>, WsError>;

	/// Called when the [WsConnection] received a JSON-RPC value, returns messages to be sent to the server or the content with parsed event name. If not the desired content and no respose is to be sent (like after a confirmation for a subscription), return a Response with an empty Vec.
	#[allow(unused_variables)]
	fn handle_jrpc(&mut self, jrpc: serde_json::Value) -> Result<ResponseOrContent, WsError>;
	//A: use this iff spot&&perp binance accept listen-key refresh through stream
	///// Additional POST communication with the exchange, not conditional on received messages, can be handled here.
	///// Really this is just for damn Binance with their stupid `listn-key` standard.
	//fn handle_post(&mut self) -> Result<Option<Vec<tungstenite::Message>>, WsError> {
	//	Ok(None)
	//}

	//#[allow(unused_variables)]
	//fn handle_jrpc(&mut self, jrpc: &serde_json::Value) -> Result<Option<Vec<tungstenite::Message>>, WsError> {
	//	Ok(None)
	//}
}

#[derive(Clone, Debug)]
pub enum ResponseOrContent {
	/// Response to a message sent to the server.
	Response(Vec<tungstenite::Message>),
	/// Content received from the server.
	Content(ContentEvent),
}
#[derive(Clone, Debug)]
pub struct ContentEvent {
	pub data: serde_json::Value,
	pub topic: String,
	pub time: Timestamp,
	pub event_type: String,
}

#[derive(Clone, Debug, Eq)]
pub struct TopicInterpreter<T> {
	/// Only one interpreter for this name is allowed to exist // enforced through `Hash` impl defined over `event_name` only
	pub event_name: String,
	/// When name matches, interpretation should succeed.
	pub interpret: fn(&serde_json::Value) -> Result<T, WsError>,
}
impl<T> std::hash::Hash for TopicInterpreter<T> {
	fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
		self.event_name.hash(state);
	}
}
impl<T> PartialEq for TopicInterpreter<T> {
	fn eq(&self, other: &Self) -> bool {
		self.event_name == other.event_name
	}
}

/// Main way to interact with the WebSocket APIs.
#[derive(Debug)]
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
	pub fn try_new(url_suffix: &str, handler: H) -> Result<Self, UrlError> {
		let config = handler.config()?;
		let url = match &config.base_url {
			Some(base_url) => base_url.join(url_suffix)?,
			None => Url::parse(url_suffix)?,
		};

		Ok(Self {
			url,
			config,
			handler,
			stream: None,
			last_reconnect_attempt: SystemTime::UNIX_EPOCH,
		})
	}

	/// The main interface. All ws operations are hidden, only thing getting through are the content messages or the lack thereof.
	pub async fn next(&mut self) -> Result<ContentEvent, WsError> {
		if let Some(inner) = &self.stream
			&& inner.connected_since + self.config.refresh_after < SystemTime::now()
		{
			tracing::info!("Refreshing connection, as `refresh_after` specified in WsConfig has elapsed ({:?})", self.config.refresh_after);
			self.reconnect().await?;
		}
		if self.stream.is_none() {
			self.connect().await?;
		}
		//- at this point self.inner is Some

		// loop until we get actual content
		let json_rpc_value = loop {
			// force a response out of the server.
			let resp = {
				let timeout = match self.stream.as_ref() {
					Some(stream) => match stream.last_unanswered_communication {
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
					},
					None => {
						tracing::error!(
							"UNEXPECTED: Stream is None at ws.rs:172 despite guard at line 163. \
							Possible causes: (1) system hibernation/sleep caused stale state, \
							(2) memory corruption, (3) logic bug in reconnection flow, \
							(4) async cancellation. \
							Last reconnect attempt: {:?} ago. Attempting to reconnect...",
							SystemTime::now().duration_since(self.last_reconnect_attempt).unwrap_or_default()
						);
						self.connect().await?;
						continue;
					}
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
						tracing::trace!("{value:#?}"); // only log it after the `handle_message` has ran, as we're assuming that if it takes any actions, it will handle logging itself. (and that will likely be at a different level of important too)
						break match { self.handler.handle_jrpc(value)? } {
							ResponseOrContent::Response(messages) => {
								self.send_all(messages).await?;
								continue; // only need to send responses when it's not yet the desired content.
							}
							ResponseOrContent::Content(content) => content,
						};
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
				Err(err) => match err {
					tungstenite::Error::ConnectionClosed => {
						tracing::error!("received `tungstenite::Error::ConnectionClosed` on polling. Will reconnect");
						self.stream = None;
						continue;
					}
					tungstenite::Error::AlreadyClosed => {
						tracing::error!("received `tungstenite::Error::AlreadyClosed` from polling. Will reconnect");
						self.stream = None;
						continue;
					}
					tungstenite::Error::Io(e) => {
						tracing::warn!("received `tungstenite::Error::Io` from polling: {e:?}. Likely indicates connection issues. Skipping.");
						continue;
					}
					tungstenite::Error::Tls(_tls_error) => todo!(),
					tungstenite::Error::Capacity(capacity_error) => {
						tracing::warn!("received `tungstenite::Error::Capacity` from polling: {capacity_error:?}. Skipping.");
						continue;
					}
					tungstenite::Error::Protocol(protocol_error) => {
						tracing::warn!("received `tungstenite::Error::Protocol` from polling: {protocol_error:?}. Will reconnect");
						self.stream = None;
						continue;
					}
					tungstenite::Error::WriteBufferFull(_) => unreachable!("can only get from writing"),
					tungstenite::Error::Utf8(e) => panic!("received `tungstenite::Error::Utf8` from polling: {e:?}. Exchange is going crazy, aborting"),
					tungstenite::Error::AttackAttempt => {
						tracing::warn!("received `tungstenite::Error::AttackAttempt` from polling. Don't have a reason to trust detection 100%, so just reconnecting.");
						self.stream = None;
						continue;
					}
					tungstenite::Error::Url(_url_error) => todo!(),
					tungstenite::Error::Http(_response) => todo!(),
					tungstenite::Error::HttpFormat(_error) => todo!(),
				},
			}
		};
		Ok(json_rpc_value)
	}

	#[instrument(skip_all)]
	async fn send_all(&mut self, messages: Vec<tungstenite::Message>) -> Result<(), tungstenite::Error> {
		if let Some(inner) = &mut self.stream {
			match messages.len() {
				0 => return Ok(()),
				1 => {
					tracing::debug!("sending to server: {:#?}", &messages[0]);
					inner.send(messages.into_iter().next().unwrap()).await?;
					inner.last_unanswered_communication = Some(SystemTime::now());
				}
				_ => {
					tracing::debug!("sending to server: {messages:#?}");
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
		self.send_all(vec![message]).await // Vec cost is negligible
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

		let auth_messages = self.handler.handle_auth()?;
		Ok(self.send_all(auth_messages).await?)
	}

	/// Sends the existing connection (if any) a `Close` message, and then simply drops it, opening a new one.
	///
	/// `pub` for testing only, does not {have to || is expected to} be exposed in any wrappers.
	pub async fn reconnect(&mut self) -> Result<(), WsError> {
		if let Some(stream) = self.stream.as_mut() {
			tracing::info!("Dropping old connection before reconnecting...");
			// Best-effort close - ignore errors since the connection may already be broken
			if let Err(e) = stream.send(tungstenite::Message::Close(None)).await {
				tracing::debug!("Failed to send Close frame (connection likely already dead): {e}");
			}
			self.stream = None;
		}
		self.connect().await
	}
}

/// Configuration for [WsHandler].
///
/// Should be returned by [WsHandler::ws_config()].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
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
	reconnect_cooldown: Duration = Duration::from_secs(3),
	/// The [WebSocketConnection] will automatically reconnect when `refresh_after` has elapsed since the last connection started.
	refresh_after: Duration = Duration::from_hours(12),
	/// A reconnection will be triggered if no messages are received within this amount of time.
	message_timeout: Duration = Duration::from_mins(16), // assume all exchanges ping more frequently than this
	/// Timeout for the response to a message sent to the server.
	///
	/// Difference from the [message_timeout](Self::message_timeout) is that here we directly request communication. Eg: sending a Ping or attempting to auth.
	response_timeout: Duration = Duration::from_mins(2),
	/// The topics that will be subscribed to on creation of the connection. Note that we don't allow for passing anything that changes state here like [Trade](Topic::Trade) payloads, thus submissions are limited to [String]s
	pub topics: HashSet<String>,
}

impl WsConfig {
	pub fn set_reconnect_cooldown(&mut self, reconnect_cooldown: Duration) -> Result<()> {
		if reconnect_cooldown.is_zero() {
			bail!("connect_cooldown must be greater than 0");
		}
		self.reconnect_cooldown = reconnect_cooldown;
		Ok(())
	}

	pub fn set_refresh_after(&mut self, refresh_after: Duration) -> Result<()> {
		if refresh_after.is_zero() {
			bail!("refresh_after must be greater than 0");
		}
		self.refresh_after = refresh_after;
		Ok(())
	}

	pub fn set_message_timeout(&mut self, message_timeout: Duration) -> Result<()> {
		if message_timeout.is_zero() {
			bail!("message_timeout must be greater than 0");
		}
		self.message_timeout = message_timeout;
		Ok(())
	}

	pub fn set_response_timout(&mut self, response_timeout: Duration) -> Result<()> {
		if response_timeout.is_zero() {
			bail!("response_timeout must be greater than 0");
		}
		self.response_timeout = response_timeout;
		Ok(())
	}
}

#[derive(Debug, derive_more::Display, thiserror::Error, derive_more::From)]
pub enum WsError {
	Definition(WsDefinitionError),
	Tungstenite(tungstenite::Error),
	Auth(AuthError),
	Parse(serde_json::Error),
	Subscription(String),
	NetworkConnection,
	Url(UrlError),
	UnexpectedEvent(serde_json::Value),
	Other(eyre::Report),
}
#[derive(Debug, derive_more::Display, thiserror::Error)]
pub enum WsDefinitionError {
	MissingUrl,
}

//DEPRECATE: or reinstate, - can't even remember what's this now
//#[derive(Debug, derive_more::Display, thiserror::Error)]
//pub enum SubscriptionError {
//	Topic(String),
//	Params(serde_json::Value),
//	Incompatible(IncompatibleSubscriptionError),
//}
//#[derive(Debug, thiserror::Error)]
//#[error("Incompatible subscription error: could not subscribe to {topic:#?} on {base_url}")]
//pub struct IncompatibleSubscriptionError {
//	topic: Topic,
//	base_url: Url,
//}

#[derive(Clone, Debug, derive_more::Display, Eq, Hash, PartialEq, serde::Serialize)]
pub enum Topic {
	String(String),
	Order(serde_json::Value),
}
