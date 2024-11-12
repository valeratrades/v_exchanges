use std::{
	collections::hash_map::{Entry, HashMap},
	mem,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
	time::Duration,
};

use futures_util::{
	sink::SinkExt,
	stream::{SplitSink, StreamExt},
};
use parking_lot::Mutex as SyncMutex;
use tokio::{
	net::TcpStream,
	sync::{mpsc as tokio_mpsc, Mutex as AsyncMutex, Notify},
	task::JoinHandle,
	time::{timeout, MissedTickBehavior},
};
use tokio_tungstenite::{tungstenite, MaybeTlsStream};
pub use tungstenite::Error as TungsteniteError;

type WebSocketStream = tokio_tungstenite::WebSocketStream<MaybeTlsStream<TcpStream>>;
type WebSocketSplitSink = SplitSink<WebSocketStream, tungstenite::Message>;

/// A `struct` that holds a websocket connection.
///
/// Dropping this `struct` terminates the connection.
///
/// # Reconnecting
/// `WebSocketConnection` automatically reconnects when an [TungsteniteError] occurs.
/// Note, that during reconnection, it is **possible** that the [WebSocketHandler] receives multiple identical messages
/// even though the message was sent only once by the server, or receives only one message even though
/// multiple identical messages were sent by the server, because there could be a time difference in the new connection and
/// the old connection.
///
/// You can use the [reconnect_state()][Self::reconnect_state()] method to check if the connection is under
/// a reconnection, or manually request a reconnection.
#[derive(Debug)]
#[must_use = "dropping WebSocketConnection closes the connection"]
pub struct WebSocketConnection<H: WebSocketHandler> {
	task_reconnect: JoinHandle<()>,
	sink: Arc<AsyncMutex<WebSocketSplitSink>>,
	inner: Arc<ConnectionInner<H>>,
	reconnect_state: ReconnectState,
}

// Two ways connections end:
// - User drops WebSocketConnection
//     1. feed_handler receives a message and closes the connection, then terminates
//     2. start_connection notices that the connection is closed, and attempts to notify feed_handler, then terminates
// - Reconnection
//     This happens when:
//     - the user requests so
//     - message timeout
//     - the server closes the connection
//     - some kind of error occurs while receiving the message
//
//     1. task_reconnect starts a new connection
//     2. task_reconnect closes the old connection
//     3. start_connection (old) notices that the connection is closed, and notifies feed_handler, then terminates
//     4. feed_handler receives the message, but ignores it because it is from the old connection
#[derive(Debug)]
struct ConnectionInner<H: WebSocketHandler> {
	url: String,
	handler: Arc<SyncMutex<H>>,
	message_tx: tokio_mpsc::UnboundedSender<(bool, FeederMessage)>,
	next_connection_id: AtomicBool,
}

enum FeederMessage {
	Message(tungstenite::Result<tungstenite::Message>),
	ConnectionClosed,
	DropConnectionRequest,
}

impl<H: WebSocketHandler> WebSocketConnection<H> {
	/// Starts a new `WebSocketConnection` to the given url using the given [handler][WebSocketHandler].
	pub async fn new(url: &str, handler: H) -> Result<Self, TungsteniteError> {
		let config = handler.websocket_config();
		let handler = Arc::new(SyncMutex::new(handler));
		let url = config.url_prefix.clone() + url;

		let (message_tx, message_rx) = tokio_mpsc::unbounded_channel();
		let reconnect_manager = ReconnectState::new();

		let connection = Arc::new(ConnectionInner {
			url,
			handler: Arc::clone(&handler),
			message_tx,
			next_connection_id: AtomicBool::new(false),
		});

		async fn feed_handler(
			connection: Arc<ConnectionInner<impl WebSocketHandler>>,
			mut message_rx: tokio_mpsc::UnboundedReceiver<(bool, FeederMessage)>,
			reconnect_manager: ReconnectState,
			config: WebSocketConfig,
			sink: Arc<AsyncMutex<WebSocketSplitSink>>,
		) {
			let mut messages: HashMap<WebSocketMessage, isize> = HashMap::new();

			let timeout_duration = if config.message_timeout.is_zero() { Duration::MAX } else { config.message_timeout };

			loop {
				match timeout(timeout_duration, message_rx.recv()).await {
					// message successfully received
					Ok(Some((id, FeederMessage::Message(Ok(message))))) => {
						// message successfully received
						if let Some(message) = WebSocketMessage::from_message(message) {
							if reconnect_manager.is_reconnecting() {
								// reconnecting
								let id_sign: isize = if id { 1 } else { -1 };
								let entry = messages.entry(message.clone());
								match entry {
									Entry::Occupied(mut occupied) => {
										if config.ignore_duplicate_during_reconnection {
											tracing::debug!("Skipping duplicate message.");
											continue;
										}

										*occupied.get_mut() += id_sign;
										if id_sign != occupied.get().signum() {
											// same message which comes from different connections, so we assume it's a duplicate.
											tracing::debug!("Skipping duplicate message.");
											continue;
										}
										// comes from the same connection, which means the message was sent twice.
									}
									Entry::Vacant(vacant) => {
										// new message
										vacant.insert(id_sign);
									}
								}
							} else {
								messages.clear();
							}
							let messages = connection.handler.lock().handle_message(message);
							let mut sink_lock = sink.lock().await;
							for message in messages {
								if let Err(error) = sink_lock.send(message.into_message()).await {
									tracing::error!("Failed to send message because of an error: {}", error);
								};
							}
							if let Err(error) = sink_lock.flush().await {
								tracing::error!("An error occurred while flushing WebSocket sink: {error:?}");
							}
						}
					}
					// failed to receive message
					Ok(Some((_, FeederMessage::Message(Err(error))))) => {
						tracing::error!("Failed to receive message because of an error: {error:?}");
						if reconnect_manager.request_reconnect() {
							tracing::info!("Reconnecting WebSocket because there was an error while receiving a message");
						}
					}
					// timeout
					Err(_) => {
						tracing::debug!("WebSocket message timeout");
						if reconnect_manager.request_reconnect() {
							tracing::info!("Reconnecting WebSocket because of timeout");
						}
					}
					// connection was closed
					Ok(Some((id, FeederMessage::ConnectionClosed))) => {
						let current_id = !connection.next_connection_id.load(Ordering::SeqCst);
						if id != current_id {
							// old connection, ignore
							continue;
						}
						tracing::debug!("WebSocket connection closed by server");
						if reconnect_manager.request_reconnect() {
							tracing::info!("Reconnecting WebSocket because it was disconnected by the server");
						}
					}
					// the connection is no longer needed because WebSocketConnection was dropped
					Ok(Some((_, FeederMessage::DropConnectionRequest))) => {
						if let Err(error) = sink.lock().await.close().await {
							tracing::debug!("Failed to close WebSocket connection: {error:?}");
						}
						break;
					}
					// message_tx has been dropped, which should never happen because it's always accessible by connection.message_tx.
					Ok(None) => unreachable!("message_rx should never be closed"),
				}
			}
			connection.handler.lock().handle_close(false);
		}

		async fn reconnect<H: WebSocketHandler>(
			interval: Duration,
			cooldown: Duration,
			connection: Arc<ConnectionInner<H>>,
			sink: Arc<AsyncMutex<WebSocketSplitSink>>,
			reconnect_manager: ReconnectState,
			no_duplicate: bool,
			wait: Duration,
		) {
			let mut cooldown = tokio::time::interval(cooldown);
			cooldown.set_missed_tick_behavior(MissedTickBehavior::Delay);
			loop {
				let timer = if interval.is_zero() {
					// never completes
					tokio::time::sleep(Duration::MAX)
				} else {
					tokio::time::sleep(interval)
				};
				tokio::select! {
					_ = reconnect_manager.inner.reconnect_notify.notified() => {},
					_ = timer => {},
				}
				tracing::debug!("Reconnection requested");
				cooldown.tick().await;
				reconnect_manager.inner.reconnecting.store(true, Ordering::SeqCst);

				// reconnect_notify might have been notified while waiting the cooldown,
				// so we consume any existing permits on reconnect_notify
				reconnect_manager.inner.reconnect_notify.notify_one();
				// this completes immediately because we just added a permit
				reconnect_manager.inner.reconnect_notify.notified().await;

				tracing::debug!("Starting reconnection process ...");
				if no_duplicate {
					tokio::time::sleep(wait).await;
				}

				// start a new connection
				match WebSocketConnection::<H>::start_connection(Arc::clone(&connection)).await {
					Ok(new_sink) => {
						// replace the sink with the new one
						let mut old_sink = mem::replace(&mut *sink.lock().await, new_sink);
						tracing::debug!("New connection established");

						if no_duplicate {
							tokio::time::sleep(wait).await;
						}

						if let Err(error) = old_sink.close().await {
							tracing::debug!("An error occurred while closing old connection: {}", error);
						}
						connection.handler.lock().handle_close(true);
						tracing::debug!("Old connection closed");
					}
					Err(error) => {
						// try reconnecting again
						tracing::error!("Failed to reconnect because of an error: {}, trying again ...", error);
						reconnect_manager.inner.reconnect_notify.notify_one();
					}
				}

				if no_duplicate {
					tokio::time::sleep(wait).await;
				}

				reconnect_manager.inner.reconnecting.store(false, Ordering::SeqCst);
				tracing::debug!("Reconnection process complete");
			}
		}

		let sink_inner = Self::start_connection(Arc::clone(&connection)).await?;
		let sink = Arc::new(AsyncMutex::new(sink_inner));

		tokio::spawn(feed_handler(Arc::clone(&connection), message_rx, reconnect_manager.clone(), config.clone(), Arc::clone(&sink)));

		let task_reconnect = tokio::spawn(reconnect(
			config.refresh_after,
			config.connect_cooldown,
			Arc::clone(&connection),
			Arc::clone(&sink),
			reconnect_manager.clone(),
			config.ignore_duplicate_during_reconnection,
			config.reconnection_wait,
		));

		Ok(Self {
			task_reconnect,
			sink,
			inner: connection,
			reconnect_state: reconnect_manager,
		})
	}

	async fn start_connection(connection: Arc<ConnectionInner<impl WebSocketHandler>>) -> Result<WebSocketSplitSink, TungsteniteError> {
		let (websocket_stream, _) = tokio_tungstenite::connect_async(connection.url.clone()).await?;
		let (mut sink, mut stream) = websocket_stream.split();

		let messages = connection.handler.lock().handle_start();
		for message in messages {
			sink.send(message.into_message()).await?;
		}
		sink.flush().await?;

		// fetch_not is unstable so we use fetch_xor
		let id = connection.next_connection_id.fetch_xor(true, Ordering::SeqCst);

		// pass messages to task_feed_handler
		tokio::spawn(async move {
			while let Some(message) = stream.next().await {
				// send the received message to the task running feed_handler
				if connection.message_tx.send((id, FeederMessage::Message(message))).is_err() {
					// the channel is closed. we can't disconnect because we don't have the sink
					tracing::debug!("WebSocket message receiver is closed; abandon connection");
					return;
				}
			}
			// the underlying WebSocket connection was closed

			drop(connection.message_tx.send((id, FeederMessage::ConnectionClosed))); // this may be Err
			tracing::debug!("WebSocket stream closed");
		});
		Ok(sink)
	}

	/// Sends a message to the connection.
	pub async fn send_message(&self, message: WebSocketMessage) -> Result<(), TungsteniteError> {
		let mut sink_lock = self.sink.lock().await;
		sink_lock.send(message.into_message()).await?;
		sink_lock.flush().await
	}

	/// Returns a [ReconnectState] for this connection.
	///
	/// See [ReconnectState] for more information.
	pub fn reconnect_state(&self) -> ReconnectState {
		self.reconnect_state.clone()
	}
}

impl<H: WebSocketHandler> Drop for WebSocketConnection<H> {
	fn drop(&mut self) {
		self.task_reconnect.abort();
		// sending None tells the feeder to close
		let current_id = !self.inner.next_connection_id.load(Ordering::SeqCst);
		self.inner.message_tx.send((current_id, FeederMessage::DropConnectionRequest)).ok();
	}
}

/// A `struct` to request the [WebSocketConnection] to perform a reconnect.
///
/// This `struct` uses an [Arc] internally, so you can obtain multiple
/// `ReconnectState`s for a single [WebSocketConnection] by [cloning][Clone].
#[derive(Debug, Clone)]
pub struct ReconnectState {
	inner: Arc<ReconnectMangerInner>,
}

#[derive(Debug)]
struct ReconnectMangerInner {
	reconnect_notify: Notify,
	reconnecting: AtomicBool,
}

impl ReconnectState {
	fn new() -> Self {
		Self {
			inner: Arc::new(ReconnectMangerInner {
				reconnect_notify: Notify::new(),
				reconnecting: AtomicBool::new(false),
			}),
		}
	}

	/// Returns `true` iff the [WebSocketConnection] is undergoing a reconnection process.
	pub fn is_reconnecting(&self) -> bool {
		self.inner.reconnecting.load(Ordering::SeqCst)
	}

	/// Request the [WebSocketConnection] to perform a reconnect.
	///
	/// Will return `false` if it is already in a reconnection process.
	pub fn request_reconnect(&self) -> bool {
		if self.is_reconnecting() {
			false
		} else {
			self.inner.reconnect_notify.notify_one();
			true
		}
	}
}

/// An enum that represents a websocket message.
///
/// See also [tungstenite::Message].
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum WebSocketMessage {
	/// A text message
	Text(String),
	/// A binary message
	Binary(Vec<u8>),
	/// A ping message
	Ping(Vec<u8>),
	/// A pong message
	Pong(Vec<u8>),
}

impl WebSocketMessage {
	fn from_message(message: tungstenite::Message) -> Option<Self> {
		match message {
			tungstenite::Message::Text(text) => Some(Self::Text(text)),
			tungstenite::Message::Binary(data) => Some(Self::Binary(data)),
			tungstenite::Message::Ping(data) => Some(Self::Ping(data)),
			tungstenite::Message::Pong(data) => Some(Self::Pong(data)),
			tungstenite::Message::Close(_) | tungstenite::Message::Frame(_) => None,
		}
	}

	fn into_message(self) -> tungstenite::Message {
		match self {
			WebSocketMessage::Text(text) => tungstenite::Message::Text(text),
			WebSocketMessage::Binary(data) => tungstenite::Message::Binary(data),
			WebSocketMessage::Ping(data) => tungstenite::Message::Ping(data),
			WebSocketMessage::Pong(data) => tungstenite::Message::Pong(data),
		}
	}
}

/// A `trait` which is used to handle events on the [WebSocketConnection].
///
/// The `struct` implementing this `trait` is required to be [Send] and ['static] because
/// it will be sent between threads.
pub trait WebSocketHandler: Send + 'static {
	/// Returns a [WebSocketConfig] that will be applied for all WebSocket connections handled by this handler.
	fn websocket_config(&self) -> WebSocketConfig {
		WebSocketConfig::default()
	}

	/// Called when a new connection has been started, and returns messages that should be sent to the server.
	///
	/// This could be called multiple times because the connection can be reconnected.
	fn handle_start(&mut self) -> Vec<WebSocketMessage> {
		tracing::debug!("WebSocket connection started");
		vec![]
	}

	/// Called when the [WebSocketConnection] received a message, returns messages to be sent to the server.
	fn handle_message(&mut self, message: WebSocketMessage) -> Vec<WebSocketMessage>;

	/// Called when a websocket connection is closed.
	///
	/// If the parameter `reconnect` is:
	/// - `true`, it means that the connection is being reconnected for some reason.
	/// - `false`, it means that the connection will not be reconnected, because the [WebSocketConnection] was dropped.
	#[allow(unused_variables)]
	fn handle_close(&mut self, reconnect: bool) {
		tracing::debug!("WebSocket connection closed; reconnect: {}", reconnect);
	}
}

/// Configuration for [WebSocketHandler].
///
/// Should be returned by [WebSocketHandler::websocket_config()].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct WebSocketConfig {
	/// Duration that should elapse between each attempt to start a new connection.
	///
	/// This matters because the [WebSocketConnection] reconnects on error. If the error
	/// continues to happen, it could spam the server if `connect_cooldown` is too short. [Default]s to 3000ms.
	pub connect_cooldown: Duration,
	/// The [WebSocketConnection] will automatically reconnect when `refresh_after` has elapsed since
	/// the last connection started. If you don't want this feature, set it to [Duration::ZERO]. [Default]s to [Duration::ZERO].
	pub refresh_after: Duration,
	/// Prefix which will be used for connections that started using this `WebSocketConfig`. [Default]s to `""`.
	///
	/// Example usage: `"wss://example.com"`
	pub url_prefix: String,
	/// During reconnection, [WebSocketHandler] might receive two identical messages
	/// even though the server sent only one message. By setting this to `true`, [WebSocketConnection]
	/// will not send duplicate messages to the [WebSocketHandler]. You should set this option to `true`
	/// when messages contain some sort of ID and are distinguishable.
	///
	/// Note, that [WebSocketConnection] will **not** check duplicate messages when it is not under reconnection
	/// even this option is set to `true`.
	pub ignore_duplicate_during_reconnection: bool,
	/// When `ignore_duplicate_during_reconnection` is set to `true`, [WebSocketConnection] will wait for a
	/// certain amount of time to make sure no message is lost. [Default]s to 300ms
	pub reconnection_wait: Duration,
	/// A reconnection will be triggered if no messages are received within this amount of time.
	/// [Default]s to [Duration::ZERO], which means no timeout will be applied.
	pub message_timeout: Duration,
}

impl WebSocketConfig {
	/// Constructs a new `WebSocketConfig` with its fields set to [default][WebSocketConfig::default()].
	pub fn new() -> Self {
		Self::default()
	}
}

impl Default for WebSocketConfig {
	fn default() -> Self {
		Self {
			connect_cooldown: Duration::from_millis(3000),
			refresh_after: Duration::ZERO,
			url_prefix: String::new(),
			ignore_duplicate_during_reconnection: false,
			reconnection_wait: Duration::from_millis(300),
			message_timeout: Duration::ZERO,
		}
	}
}
