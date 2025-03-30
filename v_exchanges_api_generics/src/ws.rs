#![feature(try_blocks)]
use std::{
	borrow::Cow,
	env,
	marker::PhantomData,
	time::{Duration, SystemTime},
	vec,
};

use futures_util::{
	SinkExt as _, StreamExt as _,
	stream::{SplitSink, SplitStream},
};
use serde_json::json;
use tokio::net::TcpStream;
use tokio_tungstenite::{
	MaybeTlsStream, WebSocketStream,
	tungstenite::{
		self, Bytes,
		client::IntoClientRequest as _,
		http::{Method, Request},
	},
};
use tracing::log::LevelFilter;
use v_utils::prelude::*;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub trait WsHandler {
	/// Return list of messages necessary to establish the connection.
	fn handle_start(&mut self) -> Vec<tungstenite::Message> {
		vec![]
	}
	/// Determines if further communication is necessary. If the message received is the desired content, returns `None`.
	fn handle_message(&mut self, message: &serde_json::Value) -> Option<Vec<tungstenite::Message>> {
		None
	}
}

#[derive(Debug)]
pub struct WsConnection<H: WsHandler> {
	url: String,
	handler: H,
	inner: Option<WsStream>,
}
impl<H: WsHandler> WsConnection<H> {
	pub fn new(url: String, handler: H) -> Self {
		let inner = None;
		Self { url, handler, inner }
	}

	/// The main interface. All ws operations are hidden, only thing getting through are the content messages or the lack thereof.
	pub async fn next(&mut self) -> Result<String, tungstenite::Error> {
		if self.inner.is_none() {
			let stream = Self::connect(&self.url, &mut self.handler).await.expect("TODO: .");
			self.inner = Some(stream);
		}

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
						//TODO!!!!!: wait configured [Duration] before reconnect
						Self::connect(&self.url, &mut self.handler).await?;
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

	async fn connect(url: &str, handler: &mut H) -> Result<WsStream, tungstenite::Error> {
		let (mut stream, http_resp) = tokio_tungstenite::connect_async(url).await?;
		tracing::debug!("Ws handshake with server: {http_resp:?}");

		let messages = handler.handle_start();
		let mut message_stream = futures_util::stream::iter(messages).map(Ok);
		stream.send_all(&mut message_stream).await?;

		Ok(stream)
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
