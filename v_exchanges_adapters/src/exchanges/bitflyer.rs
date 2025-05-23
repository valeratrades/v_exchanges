//! A module for communicating with the [bitFlyer API](https://lightning.bitflyer.com/docs).
//! For example usages, see files in the examples/ directory.

use std::{marker::PhantomData, time::SystemTime};

use hmac::{Hmac, Mac};
use rand::{Rng, distributions::Alphanumeric};
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use sha2::Sha256;
use v_exchanges_api_generics::{
	http::{header::HeaderValue, *},
	websocket::*,
};

use crate::traits::*;

/// The type returned by [Client::request()].
pub type BitFlyerRequestResult<T> = Result<T, BitFlyerRequestError>;
pub type BitFlyerRequestError = RequestError<&'static str, BitFlyerHandlerError>;

/// Options that can be set when creating handlers
pub enum BitFlyerOption {
	/// [Default] variant, does nothing
	Default,
	/// API key
	Key(String),
	/// Api secret
	Secret(SecretString),
	/// Base url for HTTP requests
	HttpUrl(BitFlyerHttpUrl),
	/// Whether [BitFlyerRequestHandler] should perform authentication
	HttpAuth(bool),
	/// [RequestConfig] used when sending requests.
	/// `url_prefix` will be overridden by [HttpUrl](Self::HttpUrl) unless `HttpUrl` is [BitFlyerHttpUrl::None].
	RequestConfig(RequestConfig),
	/// Base url for WebSocket connections
	WebSocketUrl(BitFlyerWebSocketUrl),
	/// Whether [BitFlyerWebSocketHandler] should perform authentication
	WebSocketAuth(bool),
	/// The channels to be subscribed by [BitFlyerWebSocketHandler].
	WebSocketChannels(Vec<String>),
	/// [WebSocketConfig] used for creating [WebSocketConnection]s
	/// `url_prefix` will be overridden by [WebSocketUrl](Self::WebSocketUrl) unless `WebSocketUrl` is [BitFlyerWebSocketUrl::None].
	/// By default, ignore_duplicate_during_reconnection` is set to `true`.
	WebSocketConfig(WebSocketConfig),
}

/// A `struct` that represents a set of [BitFlyerOption] s.
#[derive(Clone, derive_more::Debug)]
pub struct BitFlyerOptions {
	/// see [BitFlyerOption::Key]
	pub key: Option<String>,
	/// see [BitFlyerOption::Secret]
	#[debug("[REDACTED]")]
	pub secret: Option<SecretString>,
	/// see [BitFlyerOption::HttpUrl]
	pub http_url: BitFlyerHttpUrl,
	/// see [BitFlyerOption::HttpAuth]
	pub http_auth: bool,
	/// see [BitFlyerOption::RequestConfig]
	pub request_config: RequestConfig,
	/// see [BitFlyerOption::WebSocketUrl]
	pub websocket_url: BitFlyerWebSocketUrl,
	/// see [BitFlyerOption::WebSocketAuth]
	pub websocket_auth: bool,
	/// see [BitFlyerOption::WebSocketChannels]
	pub websocket_channels: Vec<String>,
	/// see [BitFlyerOption::WebSocketConfig]
	pub websocket_config: WebSocketConfig,
}

/// A `enum` that represents the base url of the BitFlyer HTTP API.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BitFlyerHttpUrl {
	/// `https://api.bitflyer.com`
	Main,
	/// The url will not be modified by [BitFlyerRequestHandler]
	#[default]
	None,
}

/// A `enum` that represents the base url of the BitFlyer Realtime API
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum BitFlyerWebSocketUrl {
	/// `wss://ws.lightstream.bitflyer.com`
	Default,
	/// The url will not be modified by [BitFlyerWebSocketHandler]
	None,
}

#[derive(Debug, Deserialize)]
pub struct BitFlyerChannelMessage {
	pub channel: String,
	pub message: serde_json::Value,
}

#[derive(Debug)]
pub enum BitFlyerHandlerError {
	ApiError(serde_json::Value),
	ParseError,
}

/// A `struct` that implements [RequestHandler]
pub struct BitFlyerRequestHandler<'a, R: DeserializeOwned> {
	options: BitFlyerOptions,
	_phantom: PhantomData<&'a R>,
}

/// A `struct` that implements [WebSocketHandler]
pub struct BitFlyerWebSocketHandler {
	message_handler: Box<dyn FnMut(BitFlyerChannelMessage) + Send>,
	auth_id: Option<String>,
	options: BitFlyerOptions,
}

impl<B, R> RequestHandler<B> for BitFlyerRequestHandler<'_, R>
where
	B: Serialize,
	R: DeserializeOwned,
{
	type BuildError = &'static str;
	type Successful = R;
	type Unsuccessful = BitFlyerHandlerError;

	fn base_url(&self, is_test: bool) -> String {
		match is_test {
			true => unimplemented!(),
			false => self.options.http_url.as_str().to_owned(),
		}
	}

	fn build_request(&self, mut builder: RequestBuilder, request_body: &Option<B>, _: u8) -> Result<Request, Self::BuildError> {
		if let Some(body) = request_body {
			let json = serde_json::to_vec(body).or(Err("could not serialize body as application/json"))?;
			builder = builder.header(header::CONTENT_TYPE, "application/json").body(json);
		}

		let mut request = builder.build().or(Err("failed to build request"))?;

		if self.options.http_auth {
			// https://lightning.bitflyer.com/docs?lang=en#authentication
			let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap(); // always after the epoch
			let timestamp = time.as_millis() as u64;

			let mut path = request.url().path().to_owned();
			if let Some(query) = request.url().query() {
				path.push('?');
				path.push_str(query)
			}
			let body = request.body().and_then(|body| body.as_bytes()).map(String::from_utf8_lossy).unwrap_or_default();

			let sign_contents = format!("{}{}{}{}", timestamp, request.method(), path, body);

			let secret = self.options.secret.as_ref().map(|s| s.expose_secret()).ok_or("API secret not set")?;
			let mut hmac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap(); // hmac accepts key of any length

			hmac.update(sign_contents.as_bytes());
			let signature = hex::encode(hmac.finalize().into_bytes());

			let key = HeaderValue::from_str(self.options.key.as_deref().ok_or("API key not set")?).or(Err("invalid character in API key"))?;
			let headers = request.headers_mut();
			headers.insert("ACCESS-KEY", key);
			headers.insert("ACCESS-TIMESTAMP", HeaderValue::from(timestamp));
			headers.insert("ACCESS-SIGN", HeaderValue::from_str(&signature).unwrap()); // hex digits are valid
			headers.insert(header::CONTENT_TYPE, HeaderValue::from_str("application/json").unwrap()); // only contains valid letters
		}

		Ok(request)
	}

	fn handle_response(&self, status: StatusCode, _: HeaderMap, response_body: Bytes) -> Result<Self::Successful, Self::Unsuccessful> {
		if status.is_success() {
			serde_json::from_slice(&response_body).map_err(|error| {
				tracing::debug!("Failed to parse response due to an error: {}", error);
				BitFlyerHandlerError::ParseError
			})
		} else {
			let error = match serde_json::from_slice(&response_body) {
				Ok(parsed_error) => BitFlyerHandlerError::ApiError(parsed_error),
				Err(error) => {
					tracing::debug!("Failed to parse error response due to an error: {}", error);
					BitFlyerHandlerError::ParseError
				}
			};
			Err(error)
		}
	}
}

impl WebSocketHandler for BitFlyerWebSocketHandler {
	fn websocket_config(&self) -> WebSocketConfig {
		let mut config = self.options.websocket_config.clone();
		if self.options.websocket_url != BitFlyerWebSocketUrl::None {
			config.url_prefix = self.options.websocket_url.as_str().to_owned();
		}
		config
	}

	fn handle_start(&mut self) -> Vec<WebSocketMessage> {
		if self.options.websocket_auth {
			// https://bf-lightning-api.readme.io/docs/realtime-api-auth
			if let Some(key) = self.options.key.as_deref() {
				if let Some(secret) = self.options.secret.as_ref().map(|s| s.expose_secret()) {
					let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap(); // always after the epoch
					let timestamp = time.as_millis() as u64;
					let nonce: String = rand::thread_rng().sample_iter(&Alphanumeric).take(16).map(char::from).collect();

					let mut hmac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap(); // hmac accepts key of any length

					hmac.update(format!("{timestamp}{nonce}").as_bytes());
					let signature = hex::encode(hmac.finalize().into_bytes());

					let id = format!("_auth{}", time.as_nanos());
					self.auth_id = Some(id.clone());

					return vec![WebSocketMessage::Text(
						json!({
							"method": "auth",
							"params": {
								"api_key": key,
								"timestamp": timestamp,
								"nonce": nonce,
								"signature": signature,
							},
							"id": id,
						})
						.to_string(),
					)];
				} else {
					tracing::debug!("API secret not set.");
				};
			} else {
				tracing::debug!("API key not set.");
			};
		}
		self.message_subscribe()
	}

	fn handle_message(&mut self, message: WebSocketMessage) -> Vec<WebSocketMessage> {
		#[derive(Deserialize)]
		struct Message {
			#[allow(dead_code)]
			jsonrpc: String, // 2.0
			method: Option<String>,
			result: Option<serde_json::Value>,
			params: Option<BitFlyerChannelMessage>,
			id: Option<String>,
		}

		match message {
			WebSocketMessage::Text(message) => {
				let message: Message = match serde_json::from_str(&message) {
					Ok(message) => message,
					Err(_) => {
						tracing::debug!("Invalid JSON-RPC message received");
						return vec![];
					}
				};
				if self.options.websocket_auth && self.auth_id == message.id {
					// result of auth
					if message.result == Some(serde_json::Value::Bool(true)) {
						tracing::debug!("WebSocket authentication successful");
						return self.message_subscribe();
					} else {
						tracing::error!("WebSocket authentication unsuccessful");
					}
					self.auth_id = None;
				} else if message.method.as_deref() == Some("channelMessage") {
					if let Some(channel_message) = message.params {
						(self.message_handler)(channel_message);
					}
				}
			}
			WebSocketMessage::Binary(_) => tracing::debug!("Unexpected binary message received"),
			WebSocketMessage::Ping(_) | WebSocketMessage::Pong(_) => (),
		}
		vec![]
	}
}

impl BitFlyerWebSocketHandler {
	#[inline]
	fn message_subscribe(&self) -> Vec<WebSocketMessage> {
		self.options
			.websocket_channels
			.clone()
			.into_iter()
			.map(|channel| WebSocketMessage::Text(json!({ "method": "subscribe", "params": { "channel": channel } }).to_string()))
			.collect()
	}
}

impl BitFlyerHttpUrl {
	/// The base URL that this variant represents.
	fn as_str(&self) -> &'static str {
		match self {
			Self::Main => "https://api.bitflyer.com",
			Self::None => "",
		}
	}
}

impl BitFlyerWebSocketUrl {
	/// The base URL that this variant represents.
	fn as_str(&self) -> &'static str {
		match self {
			Self::Default => "wss://ws.lightstream.bitflyer.com",
			Self::None => "",
		}
	}
}

impl HandlerOptions for BitFlyerOptions {
	type OptionItem = BitFlyerOption;

	fn update(&mut self, option: Self::OptionItem) {
		match option {
			BitFlyerOption::Default => (),
			BitFlyerOption::Key(v) => self.key = Some(v),
			BitFlyerOption::Secret(v) => self.secret = Some(v),
			BitFlyerOption::HttpUrl(v) => self.http_url = v,
			BitFlyerOption::HttpAuth(v) => self.http_auth = v,
			BitFlyerOption::RequestConfig(v) => self.request_config = v,
			BitFlyerOption::WebSocketUrl(v) => self.websocket_url = v,
			BitFlyerOption::WebSocketAuth(v) => self.websocket_auth = v,
			BitFlyerOption::WebSocketChannels(v) => self.websocket_channels = v,
			BitFlyerOption::WebSocketConfig(v) => self.websocket_config = v,
		}
	}

	fn is_authenticated(&self) -> bool {
		self.key.is_some() // some end points are satisfied with just the key, and it's really difficult to provide only a key without a secret from the clientside, so assume intent if it's missing.
	}
}

impl Default for BitFlyerOptions {
	fn default() -> Self {
		let mut websocket_config = WebSocketConfig::default();
		websocket_config.ignore_duplicate_during_reconnection = true;
		Self {
			key: None,
			secret: None,
			http_url: BitFlyerHttpUrl::Main,
			http_auth: false,
			request_config: RequestConfig::default(),
			websocket_url: BitFlyerWebSocketUrl::Default,
			websocket_auth: false,
			websocket_channels: vec![],
			websocket_config,
		}
	}
}

impl<'a, R, B> HttpOption<'a, R, B> for BitFlyerOption
where
	R: DeserializeOwned + 'a,
	B: Serialize,
{
	type RequestHandler = BitFlyerRequestHandler<'a, R>;

	fn request_handler(options: Self::Options) -> Self::RequestHandler {
		BitFlyerRequestHandler::<'a, R> { options, _phantom: PhantomData }
	}
}

impl<H: FnMut(BitFlyerChannelMessage) + Send + 'static> WebSocketOption<H> for BitFlyerOption {
	type WebSocketHandler = BitFlyerWebSocketHandler;

	fn websocket_handler(handler: H, options: Self::Options) -> Self::WebSocketHandler {
		BitFlyerWebSocketHandler {
			message_handler: Box::new(handler),
			auth_id: None,
			options,
		}
	}
}

impl HandlerOption for BitFlyerOption {
	type Options = BitFlyerOptions;
}

impl Default for BitFlyerOption {
	fn default() -> Self {
		Self::Default
	}
}
