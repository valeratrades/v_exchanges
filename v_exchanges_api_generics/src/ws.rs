use std::{
	collections::VecDeque,
	future::Future,
	pin::Pin,
	time::{Duration, SystemTime},
	vec,
};

use ahash::AHashSet;
use eyre::{Result, bail};
use futures_util::{
	FutureExt as _, SinkExt as _, StreamExt as _,
	stream::{FuturesUnordered, SplitSink, SplitStream},
};
use jiff::Timestamp;
use reqwest::Url;
use tokio::net::TcpStream;
use tokio_tungstenite::{
	MaybeTlsStream, WebSocketStream,
	tungstenite::{self, Bytes, Message},
};

use crate::{ConstructAuthError, RetryConfig, UrlError, retry::ExponentialBackoff};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsSink = SplitSink<WsStream, Message>;
type WsRead = SplitStream<WsStream>;
/// `Send + Sync` boxed FU member. `Sync` (vs the stock `BoxFuture`, which is `Send`-only) is required
/// because [WsConnection] is exposed through `ExchangeStream: Sync` downstream.
type BoxedFu = Pin<Box<dyn Future<Output = FuEvent> + Send + Sync>>;

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
	fn handle_subscribe(&mut self, topics: AHashSet<Topic>) -> Result<Vec<tungstenite::Message>, WsError>;

	/// Active-heartbeat payload, sent every [WsConfig::active_ping_freq] when that is `Some`. Some
	/// exchanges (eg Bybit) require the *client* to proactively keep the connection alive with an
	/// app-level message (`{"op":"ping"}`) rather than relying on the WebSocket protocol's ping/pong
	/// — for those, return that message here and set `active_ping_freq`. Default: no active ping.
	fn active_ping(&self) -> Vec<tungstenite::Message> {
		vec![]
	}

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
/// Main way to interact with the WebSocket APIs.
//REVIEW: manual `Debug` (was derived) — the `fu`/`sink` I/O futures aren't `Debug`; we skip them.
pub struct WsConnection<H: WsHandler> {
	url: Url,
	config: WsConfig,
	handler: H,
	backoff: ExponentialBackoff,
	/// When set, `next()` will sleep until this instant before attempting to connect.
	/// Only set on actual connection failure (not on cancellation), making `next()` cancel-safe.
	reconnect_after: Option<tokio::time::Instant>,
	/// Drives I/O concurrently: the reader is a permanent standing member; a writer is transient
	/// (0..=1). Both members own their resources (no `&self` borrow), so they survive across calls.
	fu: FuturesUnordered<BoxedFu>,
	/// The sink, parked here whenever no writer-future currently owns it.
	sink: Option<WsSink>,
	/// Ordered control / JRPC replies awaiting a flush to the server, plus any idle-timeout self-Ping.
	///
	/// Note we do NOT queue Pongs here: tungstenite auto-answers inbound Pings with a Pong carrying
	/// the exact payload (`tungstenite::protocol::WebSocket`, flushed on the next read/write), which
	/// already satisfies eg Binance's exact-echo requirement. Manually ponging would duplicate it.
	outbox: Vec<Message>,
	/// `None` == disconnected (replaces the old `stream.is_none()` check). `Some` holds the instant
	/// the live connection started, for `refresh_after`.
	connected_since: Option<SystemTime>,
	/// When we last sent something the server hasn't acknowledged (used to pick the response_ vs
	/// message_ timeout). Moved up from the old `WsConnectionStream`.
	last_unanswered_communication: Option<SystemTime>,
	/// Saw a Close / reconnecting error but returned already-collected content first; reconnect on
	/// the next `next()` call.
	pending_reconnect: bool,
	/// Surplus events from a drained batch, held so the deprecated [next_single](Self::next_single)
	/// bridge can yield one-at-a-time over the batched `next()` without dropping frames. Removed in
	/// phase 2 together with `next_single`.
	single_buffer: VecDeque<ContentEvent>,
	/// How often to fire the standing active-ping timer (`PingDue`), enqueueing the handler's
	/// [active_ping](WsHandler::active_ping) payload. `None` == no active ping (rely on inbound traffic
	/// + protocol pong, as Binance does). Copied from [WsConfig::active_ping_freq] at construction.
	active_ping_freq: Option<Duration>,
}
impl<H: WsHandler> WsConnection<H> {
	#[allow(missing_docs)]
	pub fn try_new(url_suffix: &str, handler: H) -> Result<Self, WsError> {
		let config = handler.config()?;
		let url = match &config.base_url {
			Some(base_url) => base_url.join(url_suffix).map_err(UrlError::Parse)?,
			None => Url::parse(url_suffix).map_err(UrlError::Parse)?,
		};
		let backoff = ExponentialBackoff::try_from(&config.reconnect).map_err(|e| WsError::Other(eyre::eyre!("Invalid reconnect backoff configuration: {e}")))?;
		let active_ping_freq = config.active_ping_freq;

		Ok(Self {
			url,
			config,
			handler,
			backoff,
			reconnect_after: None,
			fu: FuturesUnordered::new(),
			sink: None,
			outbox: Vec::new(),
			connected_since: None,
			last_unanswered_communication: None,
			pending_reconnect: false,
			single_buffer: VecDeque::new(),
			active_ping_freq,
		})
	}

	/// Deprecated one-event-per-call bridge over the batched [next](Self::next). Drains a whole batch
	/// internally but yields events one at a time (surplus parked in `single_buffer`), so existing
	/// adapters keep their old contract without dropping frames. Removed in phase 2.
	#[deprecated(since = "1.0.0", note = "to be switched to batched version")]
	pub async fn next_single(&mut self) -> Result<ContentEvent, WsError> {
		loop {
			if let Some(ev) = self.single_buffer.pop_front() {
				return Ok(ev);
			}
			let batch = self.next().await?;
			self.single_buffer.extend(batch);
			// A successful `next()` always yields >=1 event, so the buffer is now non-empty — the loop
			// re-enters `pop_front` and returns. The loop only spins if `next()` ever returned empty,
			// which it doesn't (it loops internally until it has content).
		}
	}

	/**
	The main interface.
	All connection upkeep (ping/pong, JRPC control replies, reconnect, refresh) is hidden; a call blocks until the socket buffer has something, then drains **all** immediately-available frames and returns every content event from them in one batch.

	cancel-safe: any path with non-empty collected content returns *before* the next `.await`, so a cancellation can only land on `self.fu.next().await` — where the half-read reader future still lives on `self.fu`, losing nothing.
	Deferred reconnect/upkeep is picked up on the next call.
	**/
	pub async fn next(&mut self) -> Result<Vec<ContentEvent>, WsError> {
		// Cancel-safe backoff: a previous failed attempt parked a target Instant; resume the wait.
		if let Some(until) = self.reconnect_after.take() {
			tokio::time::sleep_until(until).await;
		}
		if self.pending_reconnect {
			self.pending_reconnect = false;
			self.reconnect().await?;
		}
		if let Some(since) = self.connected_since
			&& since + self.config.refresh_after < SystemTime::now()
		{
			tracing::info!("Refreshing connection, as `refresh_after` specified in WsConfig has elapsed ({:?})", self.config.refresh_after);
			self.reconnect().await?;
		}
		if self.connected_since.is_none() {
			self.connect().await?;
		}

		let mut content: Vec<ContentEvent> = Vec::new();

		loop {
			self.try_flush_outbox(); // deferred upkeep flies concurrently with the read, in the same FU

			let timeout = match self.choose_timeout() {
				Some(d) => d,
				None => {
					tracing::error!("Timeout for last unanswered communication ended before `.next()` was called. This likely indicates a clientside implementation error.");
					self.reconnect().await?;
					continue;
				}
			};

			match tokio::time::timeout(timeout, self.fu.next()).await {
				Err(_) => {
					// Nothing arrived in time. Return any collected content first; defer ping/reconnect.
					if !content.is_empty() {
						return Ok(content);
					}
					if self.last_unanswered_communication.is_some() {
						tracing::warn!("Response to a forced communication timed out after {timeout:?}. Reconnecting.");
						self.reconnect().await?;
					} else {
						// Standard message_timeout elapsed with nothing pending, — force life out of the server with our own Ping (queued; flushed on the next loop's `try_flush_outbox`).
						self.outbox.push(Message::Ping(Bytes::default()));
					}
					continue;
				}
				Ok(None) => {
					// FU drained (defensive; the reader is a permanent member so this shouldn't happen).
					tracing::warn!("FuturesUnordered empty despite permanent reader. Reconnecting.");
					self.reconnect().await?;
					continue;
				}
				Ok(Some(FuEvent::Write { sink, result })) => {
					self.sink = Some(sink); // park the sink back
					if let Err(e) = result
						&& is_reconnecting(&e)
					{
						tracing::warn!("Write failed ({e:?}). Reconnecting.");
						self.reconnect().await?;
					}
					continue; // a write is never content
				}
				Ok(Some(FuEvent::PingDue)) => {
					// Active heartbeat fell due: queue the handler's ping payload (flushed by the next loop's `try_flush_outbox`) and re-arm the standing timer.
					// Treated like any other outbound action — it sets `last_unanswered_communication` on flush, so a missed reply trips the response-timeout reconnect just as a self-Ping would.
					self.outbox.extend(self.handler.active_ping());
					self.arm_ping();
					continue; // a ping is never content
				}
				Ok(Some(FuEvent::Read { reader, batch })) => {
					if batch.is_empty() {
						// EOF.
						drop(reader);
						if !content.is_empty() {
							self.pending_reconnect = true;
							return Ok(content);
						}
						tracing::warn!("tungstenite read EOF from the stream. Reconnecting.");
						self.reconnect().await?;
						continue;
					}
					self.arm_reader(reader); // re-arm the standing member NOW
					self.last_unanswered_communication = None; // heard from the server

					let mut terminal = false; // saw Close / a reconnecting error
					for frame in batch {
						let __pong_ack = || tracing::trace!("Received app-level pong (active-ping ack)");
						match frame {
							Ok(Message::Text(text)) => {
								let value: serde_json::Value =
									serde_json::from_str(&text).expect("API sent invalid JSON, which is completely unexpected. Disappointment is immeasurable and the day is ruined.");
								// App-level heartbeat ack to our active-ping (Bybit et al. answer `{"op":"ping"}` with a text-frame `{"op":"pong"}` / `{"ret_msg":"pong"}`, NOT a protocol Pong).
								// It carries no content and needs no reply — drop it before it reaches the handler, whose strict response parse doesn't model a pong.
								if value["op"] == "pong" || value["ret_msg"] == "pong" {
									__pong_ack();
									continue;
								}
								tracing::trace!("{value:#?}");
								match self.handler.handle_jrpc(value)? {
									ResponseOrContent::Response(messages) => self.outbox.extend(messages),
									ResponseOrContent::Content(c) => content.push(c),
								}
							}
							// tungstenite already queued a Pong with this exact payload (auto-answered on read, flushed on the next read/write), — (satisfying even Binance's exact-echo rule).
							// No need to pong ourselves or the server receives two Pongs per Ping.
							Ok(Message::Ping(_)) => tracing::trace!("Received ping (tungstenite auto-pongs)"),
							Ok(Message::Pong(_)) => __pong_ack(),
							Ok(Message::Close(maybe_reason)) => {
								match maybe_reason {
									Some(close_frame) => tracing::info!("Server closed connection; reason: {close_frame:?}"),
									None => tracing::info!("Server closed connection; no reason specified."),
								}
								terminal = true;
								break;
							}
							Ok(Message::Binary(_)) => panic!("Received binary. But exchanges are not smart enough to send this, what is happening"),
							Ok(Message::Frame(_)) => unreachable!("Can't get from reading"),
							Err(e) if is_reconnecting(&e) => {
								tracing::warn!("Reconnecting-class error mid-batch: {e:?}");
								terminal = true;
								break;
							}
							// Non-reconnecting class: `is_reconnecting` already panicked on Utf8 / unreachable on the write-only variant. The remainder (Capacity) is skippable.
							Err(e) => {
								debug_assert!(!is_reconnecting(&e));
								tracing::warn!("Skipping non-fatal polling error: {e:?}");
							}
						}
					}

					if terminal {
						// Reconnecting class: any queued writes target a soon-dead connection -> discard.
						self.outbox.clear();
						if !content.is_empty() {
							self.pending_reconnect = true; // reconnect on the next call
							return Ok(content); // content-before-Close returned first, never lost
						}
						self.reconnect().await?;
						continue;
					}
					if !content.is_empty() {
						return Ok(content);
					}
					continue; // only upkeep parsed -> keep awaiting the FU
				}
			}
		}
	}

	/// Push the permanent standing reader future onto the FU.
	fn arm_reader(&mut self, reader: WsRead) {
		self.fu.push(Box::pin(read_future(reader)));
	}

	/// Push the standing active-ping timer onto the FU, IF one is configured. No-op when
	/// `active_ping_freq` is `None` (no heartbeat needed — the reader stays the only standing member).
	fn arm_ping(&mut self) {
		if let Some(freq) = self.active_ping_freq {
			self.fu.push(Box::pin(ping_future(freq)));
		}
	}

	/// Launch a writer-future IF the sink is parked and there's queued work. Does nothing if a writer
	/// is already in flight (sink absent) or the outbox is empty.
	fn try_flush_outbox(&mut self) {
		if self.sink.is_none() {
			return; // a writer is already in flight
		}
		if self.outbox.is_empty() {
			return;
		}
		let sink = self.sink.take().expect("guarded `is_none` above");
		let msgs = std::mem::take(&mut self.outbox);
		tracing::debug!("flushing to server: {msgs:#?}");
		self.last_unanswered_communication = Some(SystemTime::now());
		self.fu.push(Box::pin(write_future(sink, msgs)));
	}

	/// Pick the read timeout based on whether we're awaiting a forced response. `None` means a forced
	/// response's window already elapsed before this call — the caller reconnects.
	fn choose_timeout(&self) -> Option<Duration> {
		match self.last_unanswered_communication {
			Some(last_unanswered) => match last_unanswered + self.config.response_timeout > SystemTime::now() {
				true => Some(self.config.response_timeout),
				false => None,
			},
			None => Some(self.config.message_timeout),
		}
	}

	async fn connect(&mut self) -> Result<(), WsError> {
		tracing::info!("Connecting to {}...", self.url);

		let (stream, http_resp) = match tokio_tungstenite::connect_async(self.url.as_str()).await {
			Ok(result) => result,
			Err(e) => {
				let delay = self.backoff.next_duration();
				if !delay.is_zero() {
					tracing::warn!(delay_ms = delay.as_millis(), "Connection failed, backing off before retry.");
					self.reconnect_after = Some(tokio::time::Instant::now() + delay);
				}
				return Err(e.into());
			}
		};
		tracing::debug!("Ws handshake with server: {http_resp:#?}");

		let (sink, reader) = stream.split();
		self.fu = FuturesUnordered::new();
		self.sink = Some(sink);
		self.arm_reader(reader);
		self.arm_ping(); // standing heartbeat timer (no-op if `active_ping_freq` is None)
		self.outbox.clear();
		self.last_unanswered_communication = None;
		self.connected_since = Some(SystemTime::now());

		// Auth/subscribe messages are *enqueued*, not inline-sent: the flush flies on the FU like any other write, concurrently with the standing read.
		let auth_messages = self.handler.handle_auth()?;
		self.outbox.extend(auth_messages);
		self.try_flush_outbox();

		self.reconnect_after = None;
		self.backoff.reset();
		Ok(())
	}

	/// Best-effort `Close` the existing connection (if its sink is parked), drop everything, and open
	/// a new one. If a writer currently owns the sink, we skip the Close and just drop the FU — the
	/// same best-effort contract as before.
	///
	/// `pub` for testing only, does not {have to || is expected to} be exposed in any wrappers.
	pub async fn reconnect(&mut self) -> Result<(), WsError> {
		// Clear any pending backoff — a server-initiated reconnect should be attempted immediately.
		// If the new connection fails, `connect()` will set a fresh backoff.
		self.reconnect_after = None;
		if let Some(mut sink) = self.sink.take() {
			tracing::info!("Dropping old connection before reconnecting...");
			// Best-effort close - ignore errors since the connection may already be broken.
			if let Err(e) = sink.send(Message::Close(None)).await {
				tracing::debug!("Failed to send Close frame (connection likely already dead): {e}");
			}
		}
		self.fu = FuturesUnordered::new(); // drops the reader/writer futures + the old split halves
		self.connected_since = None;
		self.connect().await
	}
}

/// Configuration for [WsHandler].
///
/// Should be returned by [WsHandler::ws_config()].
#[derive(Clone, Debug)]
pub struct WsConfig {
	/// Whether the connection should be authenticated. Normally implemented through a "listen key"
	pub auth: bool,
	/// Prefix which will be used for connections that started using this `WebSocketConfig`.
	///
	/// Ex: `"wss://example.com"`
	pub base_url: Option<Url>,
	/// Backoff configuration for reconnect attempts.
	pub reconnect: RetryConfig,
	/// The [WebSocketConnection] will automatically reconnect when `refresh_after` has elapsed since the last connection started.
	refresh_after: Duration,
	/// A reconnection will be triggered if no messages are received within this amount of time.
	message_timeout: Duration,
	/// Timeout for the response to a message sent to the server.
	///
	/// Difference from the [message_timeout](Self::message_timeout) is that here we directly request communication. Eg: sending a Ping or attempting to auth.
	response_timeout: Duration,
	/// The topics that will be subscribed to on creation of the connection. Note that we don't allow for passing anything that changes state here like [Trade](Topic::Trade) payloads, thus submissions are limited to [String]s
	pub topics: AHashSet<String>,
	/// How often the [WsConnection] proactively sends the handler's [active_ping](WsHandler::active_ping)
	/// payload. `None` (default) == no active ping: rely on inbound traffic + protocol pong (Binance).
	/// `Some(d)` == fire every `d` regardless of inbound traffic — required by exchanges like Bybit that
	/// drop a connection unless the *client* sends an app-level `{"op":"ping"}` within a fixed window.
	active_ping_freq: Option<Duration>,
}
impl WsConfig {
	pub fn set_reconnect(&mut self, reconnect: RetryConfig) {
		self.reconnect = reconnect;
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

	pub fn set_active_ping_freq(&mut self, active_ping_freq: Duration) -> Result<()> {
		if active_ping_freq.is_zero() {
			bail!("active_ping_freq must be greater than 0");
		}
		self.active_ping_freq = Some(active_ping_freq);
		Ok(())
	}
}

#[derive(Debug, miette::Diagnostic, derive_more::Display, thiserror::Error, derive_more::From)]
pub enum WsError {
	#[diagnostic(transparent)]
	Definition(WsDefinitionError),
	#[diagnostic(code(v_exchanges::ws::tungstenite), help("WebSocket protocol error. The connection may need to be reestablished."))]
	Tungstenite(tungstenite::Error),
	#[diagnostic(transparent)]
	Auth(ConstructAuthError),
	#[diagnostic(code(v_exchanges::ws::parse), help("Failed to parse WebSocket message. Check if the exchange API has changed."))]
	Parse(serde_json::Error),
	#[diagnostic(code(v_exchanges::ws::subscription))]
	Subscription(String),
	#[diagnostic(code(v_exchanges::ws::network), help("Network connection failed. Check your internet connection."))]
	NetworkConnection,
	#[diagnostic(transparent)]
	Url(UrlError),
	#[diagnostic(code(v_exchanges::ws::unexpected_event), help("Received an unexpected event from the WebSocket. This may indicate an API change."))]
	UnexpectedEvent(serde_json::Value),
	#[error(transparent)]
	Other(eyre::Report),
}
#[derive(Debug, miette::Diagnostic, derive_more::Display, thiserror::Error)]
pub enum WsDefinitionError {
	#[diagnostic(code(v_exchanges::ws::definition::missing_url), help("WebSocket base URL must be configured in WsConfig."))]
	MissingUrl,
}
#[derive(Clone, Debug, derive_more::Display, Eq, Hash, PartialEq, serde::Serialize)]
pub enum Topic {
	String(String),
	Order(serde_json::Value),
}
/// Homogeneous [FuturesUnordered] members for [WsConnection]; both own their resources, so each
/// future is `'static` and holds no `&self` borrow — letting it live on the struct across `next()`
/// calls (which is what makes the draining `next()` cancel-safe).
enum FuEvent {
	/// The permanent standing reader: blocks for the first frame, then drains all immediately-
	/// available ones. `batch` empty == EOF sentinel.
	Read { reader: WsRead, batch: Vec<Result<Message, tungstenite::Error>> },
	/// A transient writer (0..=1 in flight): owned the sink + queued messages, sent them, hands the
	/// sink back via this event.
	Write { sink: WsSink, result: Result<(), tungstenite::Error> },
	/// The (optional) standing active-ping timer: sleeps `active_ping_freq`, then fires. Re-armed on
	/// every fire, so it ticks for the whole connection lifetime. Carries no resources — the handler
	/// supplies the actual ping payload when it fires.
	PingDue,
}

/// Block for the first frame, then drain everything already buffered without blocking.
///
/// `now_or_never` polls the RAW reader once with a no-op waker: `Some(frame)` -> keep & continue,
/// `None` -> nothing immediately ready -> stop. Safe because a raw `reader.next()` is cancel-safe at
/// the frame boundary (a `Pending` poll consumes no frame, loses nothing). This is NOT a kernel
/// "read all" call (tungstenite / `futures::Stream` expose no such API and one can't exist cleanly);
/// it drains what's cheaply available — tungstenite's parsed `in_buffer` plus whatever one chunk-read
/// pulled (~1 syscall for many frames). Any straggler still in the kernel simply becomes the first
/// frame of the next `next()` call, which is what we want (don't block to chase stragglers).
async fn read_future(mut reader: WsRead) -> FuEvent {
	let mut batch = Vec::new();
	if let Some(first) = reader.next().await {
		batch.push(first);
		while let Some(Some(m)) = reader.next().now_or_never() {
			batch.push(m);
		}
	} // else: empty batch == EOF sentinel
	FuEvent::Read { reader, batch }
}

/// Send every queued message, then hand the sink back via the returned event.
async fn write_future(mut sink: WsSink, msgs: Vec<Message>) -> FuEvent {
	let mut s = futures_util::stream::iter(msgs).map(Ok);
	let result = sink.send_all(&mut s).await;
	FuEvent::Write { sink, result }
}

/// Sleep one active-ping interval, then fire. Owns no resources — re-armed by the caller on each fire.
async fn ping_future(freq: Duration) -> FuEvent {
	tokio::time::sleep(freq).await;
	FuEvent::PingDue
}

/// Whether a polling error from tungstenite warrants tearing down & reconnecting.
///
/// Folds the per-variant arms of the old single-frame `next()` match: the connection-fatal classes
/// reconnect; `Capacity` is skippable (false); `Utf8` panics at the call site; the TLS/URL/HTTP
/// classes are still `todo!()` as before; the write-only variants are unreachable on a read.
fn is_reconnecting(err: &tungstenite::Error) -> bool {
	match err {
		tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed | tungstenite::Error::Io(_) | tungstenite::Error::Protocol(_) | tungstenite::Error::AttackAttempt => true,
		tungstenite::Error::Capacity(_) => false,
		tungstenite::Error::Utf8(e) => panic!("received `tungstenite::Error::Utf8` from polling: {e:?}. Exchange is going crazy, aborting"),
		tungstenite::Error::WriteBufferFull(_) => unreachable!("can only get from writing"),
		tungstenite::Error::Tls(_) | tungstenite::Error::Url(_) | tungstenite::Error::Http(_) | tungstenite::Error::HttpFormat(_) => todo!(),
	}
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

impl<H: WsHandler> std::fmt::Debug for WsConnection<H> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("WsConnection")
			.field("url", &self.url)
			.field("config", &self.config)
			.field("handler", &self.handler)
			.field("backoff", &self.backoff)
			.field("reconnect_after", &self.reconnect_after)
			.field("connected_since", &self.connected_since)
			.field("last_unanswered_communication", &self.last_unanswered_communication)
			.field("pending_reconnect", &self.pending_reconnect)
			.field("outbox_len", &self.outbox.len())
			.field("active_ping_freq", &self.active_ping_freq)
			.finish_non_exhaustive()
	}
}

// legacy WsConnection {{{1
/// Verbatim snapshot of the pre-batch `WsConnection`, kept side-by-side purely so the original
/// single-frame `next_single` (and its `send`/`connect`/`reconnect` plumbing over a whole
/// [WsStream], without the split sink/reader + [FuturesUnordered](futures_util::stream::FuturesUnordered))
/// can be compared against the new batched design. Not wired into any adapter; delete the whole
/// module once the batched path is fully trusted.
#[deprecated(since = "1.0.0", note = "here for ref. Remove when certain that the new batched design is solid.")]
mod legacy {
	use super::*;

	/// See the [module docs](self).
	#[derive(Debug)]
	pub struct WsConnectionLegacy<H: WsHandler> {
		url: Url,
		config: WsConfig,
		handler: H,
		stream: Option<WsConnectionStream>,
		backoff: ExponentialBackoff,
		/// When set, `next()` will sleep until this instant before attempting to connect.
		/// Only set on actual connection failure (not on cancellation), making `next()` cancel-safe.
		reconnect_after: Option<tokio::time::Instant>,
	}
	impl<H: WsHandler> WsConnectionLegacy<H> {
		#[allow(missing_docs)]
		pub fn try_new(url_suffix: &str, handler: H) -> Result<Self, WsError> {
			let config = handler.config()?;
			let url = match &config.base_url {
				Some(base_url) => base_url.join(url_suffix).map_err(UrlError::Parse)?,
				None => Url::parse(url_suffix).map_err(UrlError::Parse)?,
			};
			let backoff = ExponentialBackoff::try_from(&config.reconnect).map_err(|e| WsError::Other(eyre::eyre!("Invalid reconnect backoff configuration: {e}")))?;

			Ok(Self {
				url,
				config,
				handler,
				stream: None,
				backoff,
				reconnect_after: None,
			})
		}

		/// The main interface. All ws operations are hidden, only thing getting through are the content messages or the lack thereof.
		pub async fn next_single(&mut self) -> Result<ContentEvent, WsError> {
			// Cancel-safe backoff: if a previous connection attempt failed, sleep until the backoff
			// period expires. Stored as an Instant so cancellation (e.g. by tokio::select!) preserves
			// the target time — next call resumes the remaining wait rather than restarting it.
			if let Some(until) = self.reconnect_after {
				tokio::time::sleep_until(until).await;
				self.reconnect_after = None;
			}

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
							Backoff current delay: {:?}. Attempting to reconnect...",
								self.backoff.current_delay()
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
							tracing::warn!("Message reception timed out after {timeout:?} seconds. // {timeout_error}");
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
							tracing::error!("received `tungstenite::Error::Io` from polling: {e:?}. Atm don't know valid cases of this happening given intact application state...");
							self.stream = None;
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

		#[tracing::instrument(skip_all)]
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

			let (stream, http_resp) = match tokio_tungstenite::connect_async(self.url.as_str()).await {
				Ok(result) => result,
				Err(e) => {
					let delay = self.backoff.next_duration();
					if !delay.is_zero() {
						tracing::warn!(delay_ms = delay.as_millis(), "Connection failed, backing off before retry.");
						self.reconnect_after = Some(tokio::time::Instant::now() + delay);
					}
					return Err(e.into());
				}
			};
			tracing::debug!("Ws handshake with server: {http_resp:#?}");

			let now = SystemTime::now();
			self.stream = Some(WsConnectionStream::new(stream, now));

			let auth_messages = self.handler.handle_auth()?;
			self.send_all(auth_messages).await?;
			self.reconnect_after = None;
			self.backoff.reset();
			Ok(())
		}

		/// Sends the existing connection (if any) a `Close` message, and then simply drops it, opening a new one.
		///
		/// `pub` for testing only, does not {have to || is expected to} be exposed in any wrappers.
		pub async fn reconnect(&mut self) -> Result<(), WsError> {
			// Clear any pending backoff — a server-initiated reconnect should be attempted immediately.
			// If the new connection fails, `connect()` will set a fresh backoff.
			self.reconnect_after = None;
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

	/// State for the legacy [WsConnectionLegacy] only: the whole [WsStream] behind a `Deref`, plus the
	/// connection-age / unanswered-communication bookkeeping the batched path now keeps inline on the
	/// connection itself.
	#[derive(Debug, derive_more::Deref, derive_more::DerefMut, derive_new::new)]
	struct WsConnectionStream {
		#[deref_mut]
		#[deref]
		stream: WsStream,
		connected_since: SystemTime,
		#[new(default)]
		last_unanswered_communication: Option<SystemTime>,
	}
}
//,}}}1

impl Default for WsConfig {
	fn default() -> Self {
		Self {
			auth: false,
			base_url: None,
			reconnect: RetryConfig {
				max_retries: u32::MAX,
				initial_delay_ms: 1_000,
				max_delay_ms: 30_000,
				backoff_factor: 2.0,
				jitter_ms: 500,
				immediate_first: false,
				max_elapsed_ms: None,
			},
			refresh_after: Duration::from_hours(12),
			message_timeout: Duration::from_mins(16),
			response_timeout: Duration::from_mins(2),
			topics: AHashSet::new(),
			active_ping_freq: None,
		}
	}
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

#[cfg(test)]
mod tests {
	use futures_util::SinkExt as _;
	use tokio::net::TcpListener;
	use tokio_tungstenite::accept_async;

	use super::*;

	/// Trivial handler for the in-process server: every text frame is content (`{"n": <i>}`), nothing
	/// is ever replied. Short timeouts keep the hermetic tests sub-second.
	#[derive(Debug)]
	struct EchoHandler;
	impl WsHandler for EchoHandler {
		fn config(&self) -> Result<WsConfig, UrlError> {
			let mut c = WsConfig::default();
			// Short timeouts keep the hermetic tests sub-second. `.expect`: literals are non-zero.
			c.set_message_timeout(Duration::from_millis(200)).expect("non-zero literal");
			c.set_response_timout(Duration::from_millis(200)).expect("non-zero literal");
			Ok(c)
		}

		fn handle_subscribe(&mut self, _topics: AHashSet<Topic>) -> Result<Vec<Message>, WsError> {
			Ok(vec![])
		}

		fn handle_jrpc(&mut self, jrpc: serde_json::Value) -> Result<ResponseOrContent, WsError> {
			Ok(ResponseOrContent::Content(ContentEvent {
				data: jrpc.clone(),
				topic: "test".to_owned(),
				time: Timestamp::UNIX_EPOCH,
				event_type: "test".to_owned(),
			}))
		}
	}

	/// Like [EchoHandler] but with a fast active-ping configured, emitting Bybit-style `{"op":"ping"}`.
	/// Used to exercise the standing `PingDue` timer and the upstream pong-ack drop.
	#[derive(Debug)]
	struct PingHandler;
	impl WsHandler for PingHandler {
		fn config(&self) -> Result<WsConfig, UrlError> {
			let mut c = WsConfig::default();
			// Fire the active-ping fast; keep read timeouts long so the test ends by our own logic, not a
			// message_timeout. `.expect`: literals are non-zero.
			c.set_active_ping_freq(Duration::from_millis(80)).expect("non-zero literal");
			c.set_message_timeout(Duration::from_secs(5)).expect("non-zero literal");
			c.set_response_timout(Duration::from_secs(5)).expect("non-zero literal");
			Ok(c)
		}

		fn active_ping(&self) -> Vec<Message> {
			vec![Message::Text("{\"op\":\"ping\"}".into())]
		}

		// Would wrongly surface ANY text as content — so a pong-ack reaching here fails the drop test.
		fn handle_subscribe(&mut self, _topics: AHashSet<Topic>) -> Result<Vec<Message>, WsError> {
			Ok(vec![])
		}

		fn handle_jrpc(&mut self, jrpc: serde_json::Value) -> Result<ResponseOrContent, WsError> {
			Ok(ResponseOrContent::Content(ContentEvent {
				data: jrpc,
				topic: "test".to_owned(),
				time: Timestamp::UNIX_EPOCH,
				event_type: "test".to_owned(),
			}))
		}
	}

	/// Bind an ephemeral loopback port, returning `(listener, "ws://127.0.0.1:<port>")`.
	async fn bind() -> (TcpListener, String) {
		let listener = TcpListener::bind("127.0.0.1:0").await.expect("loopback bind");
		let url = format!("ws://{}", listener.local_addr().unwrap());
		(listener, url)
	}

	/// Drain: the server writes N text frames back-to-back (one buffered flush so they land together
	/// over loopback), then goes quiet → a single `next()` must return all N (whole-buffer drain in one
	/// call, not one-per-call).
	#[tokio::test]
	async fn drains_whole_buffer_in_one_call() {
		const N: usize = 8;
		let (listener, url) = bind().await;

		let server = async move {
			let (tcp, _) = listener.accept().await.expect("accept");
			let mut ws = accept_async(tcp).await.expect("handshake");
			// `feed` all then a single `flush`: coalesces into one write so the frames arrive as one
			// readable chunk and the client's `now_or_never` drain sees them all.
			for i in 0..N {
				ws.feed(Message::Text(format!("{{\"n\":{i}}}").into())).await.expect("feed");
			}
			ws.flush().await.expect("flush");
			// Stay quiet but keep the connection open so the client doesn't see EOF.
			tokio::time::sleep(Duration::from_secs(3)).await;
			drop(ws);
		};
		let handle = tokio::spawn(server);

		let mut conn = WsConnection::try_new(&url, EchoHandler).expect("try_new");
		let batch = conn.next().await.expect("next");
		assert_eq!(batch.len(), N, "expected all {N} buffered frames drained in a single next()");
		handle.abort();
	}

	/// Side-effects mid-drain (constraint: never drop ping handling): server sends `[Ping, Text, Text]`
	/// → the call returns 2 content events AND the server observes exactly one Pong.
	///
	/// The Pong is tungstenite's automatic reply to the inbound Ping (carrying the exact payload),
	/// flushed when our reader is next polled — NOT a manual pong (manual would duplicate it).
	#[tokio::test]
	async fn pongs_once_and_returns_content_from_same_batch() {
		let (listener, url) = bind().await;

		let server = async move {
			let (tcp, _) = listener.accept().await.expect("accept");
			let mut ws = accept_async(tcp).await.expect("handshake");
			ws.feed(Message::Ping(Bytes::from_static(b"hi"))).await.expect("feed ping");
			ws.feed(Message::Text("{\"n\":1}".into())).await.expect("feed t1");
			ws.feed(Message::Text("{\"n\":2}".into())).await.expect("feed t2");
			ws.flush().await.expect("flush");
			// Count every Pong the client sends until it disconnects (we drop it client-side below).
			let mut pongs = 0usize;
			while let Some(msg) = ws.next().await {
				match msg {
					Ok(Message::Pong(_)) => pongs += 1,
					Ok(Message::Close(_)) | Err(_) => break,
					_ => {}
				}
			}
			pongs
		};
		let handle = tokio::spawn(server);

		let mut conn = WsConnection::try_new(&url, EchoHandler).expect("try_new");
		let batch = conn.next().await.expect("next");
		assert_eq!(batch.len(), 2, "two text frames in the batch become two content events");

		// Drive one more bounded `next()`: it re-polls the (re-armed) reader, which flushes the queued
		// auto-pong, then blocks on the now-quiet socket and times out. We only care that the Pong went
		// out exactly once. Then drop the client so the server's read loop ends and yields its count.
		// Bounded drive purely for its side-effect (flush the auto-pong); the timeout-elapsed result
		// is expected and discarded.
		let _drove_flush = tokio::time::timeout(Duration::from_millis(150), conn.next()).await;
		drop(conn);
		let pongs = tokio::time::timeout(Duration::from_secs(1), handle).await.expect("server join timeout").expect("server task");
		assert_eq!(pongs, 1, "exactly one Pong (tungstenite's auto-reply) for the single Ping");
	}

	/// Terminal: server sends `[Text, Close]` → `next()` returns `Ok(vec![one])` (content before Close
	/// kept, not lost, not errored). The next call must reconnect (observed as a second accept).
	#[tokio::test]
	async fn returns_content_before_close_then_reconnects() {
		let (listener, url) = bind().await;

		let server = async move {
			// First connection: one text then a Close.
			let (tcp, _) = listener.accept().await.expect("accept 1");
			let mut ws = accept_async(tcp).await.expect("handshake 1");
			ws.feed(Message::Text("{\"n\":1}".into())).await.expect("feed");
			ws.feed(Message::Close(None)).await.expect("feed close");
			ws.flush().await.expect("flush");
			drop(ws);

			// Second connection proves the client reconnected after the Close.
			let (tcp2, _) = listener.accept().await.expect("accept 2 (reconnect)");
			let mut ws2 = accept_async(tcp2).await.expect("handshake 2");
			ws2.feed(Message::Text("{\"n\":2}".into())).await.expect("feed 2");
			ws2.flush().await.expect("flush 2");
			tokio::time::sleep(Duration::from_secs(2)).await;
		};
		let handle = tokio::spawn(server);

		let mut conn = WsConnection::try_new(&url, EchoHandler).expect("try_new");
		let first = conn.next().await.expect("next 1");
		assert_eq!(first.len(), 1, "content collected before Close is returned, not dropped");

		// Next call reconnects (pending_reconnect was set) and yields the second connection's frame.
		let second = conn.next().await.expect("next 2 after reconnect");
		assert_eq!(second.len(), 1, "after reconnect we receive the new connection's frame");
		handle.abort();
	}

	/// Active-ping: with `active_ping_freq` set, the client must proactively send the handler's
	/// `{"op":"ping"}` payload on a quiet connection (no inbound traffic), within the configured window.
	#[tokio::test]
	async fn sends_active_ping_on_quiet_connection() {
		let (listener, url) = bind().await;

		let server = async move {
			let (tcp, _) = listener.accept().await.expect("accept");
			let mut ws = accept_async(tcp).await.expect("handshake");
			// Stay silent; just wait for the client's first app-level ping text frame.
			loop {
				match ws.next().await {
					Some(Ok(Message::Text(t))) if t.contains("\"op\":\"ping\"") => return true,
					Some(Ok(_)) => continue, // ignore protocol pings/pongs etc.
					_ => return false,       // closed/errored before any app ping
				}
			}
		};
		let handle = tokio::spawn(server);

		let mut conn = WsConnection::try_new(&url, PingHandler).expect("try_new");
		// Drive `next()` long enough for the 80ms ping timer to fire and flush; it returns no content
		// (only the ping flies), so the timeout-elapsed result is expected and discarded.
		let _drove_ping = tokio::time::timeout(Duration::from_millis(400), conn.next()).await;
		let saw_ping = tokio::time::timeout(Duration::from_secs(1), handle).await.expect("server join timeout").expect("server task");
		assert!(saw_ping, "client must send an app-level `{{\"op\":\"ping\"}}` on a quiet connection");
	}

	/// Pong-ack drop: a text-frame `{"op":"pong"}` (the heartbeat reply some exchanges send instead of a
	/// protocol Pong) must be swallowed upstream — never surfaced as content, even though `PingHandler`
	/// would wrongly turn any text into content if it ever reached `handle_jrpc`.
	#[tokio::test]
	async fn drops_app_level_pong_ack() {
		let (listener, url) = bind().await;

		let server = async move {
			let (tcp, _) = listener.accept().await.expect("accept");
			let mut ws = accept_async(tcp).await.expect("handshake");
			// A pong-ack, then a real content frame. Only the latter may surface to the caller.
			ws.feed(Message::Text("{\"op\":\"pong\"}".into())).await.expect("feed pong");
			ws.feed(Message::Text("{\"n\":1}".into())).await.expect("feed content");
			ws.flush().await.expect("flush");
			tokio::time::sleep(Duration::from_secs(2)).await;
		};
		let handle = tokio::spawn(server);

		let mut conn = WsConnection::try_new(&url, PingHandler).expect("try_new");
		let batch = conn.next().await.expect("next");
		assert_eq!(batch.len(), 1, "pong-ack dropped upstream; only the real content frame surfaces");
		assert_eq!(batch[0].data["n"], 1, "the surviving event is the content frame, not the pong");
		handle.abort();
	}
}
