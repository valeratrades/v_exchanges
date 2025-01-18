// A module for communicating with the MEXC API (https://mexcdevelop.github.io/apidocs/spot/en/)

use std::{
	marker::PhantomData,
	str::FromStr,
	time::{Duration, SystemTime},
};

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::Sha256;
use v_exchanges_api_generics::{http::*, websocket::*};

use crate::traits::*;

/// The type returned by Client::request().
pub type MexcRequestResult<T> = Result<T, MexcRequestError>;
pub type MexcRequestError = RequestError<&'static str, MexcHandlerError>;

/// Options that can be set when creating handlers
pub enum MexcOption {
	/// [Default] variant, does nothing
	Default,
	/// API key
	Key(String),
	/// Api secret
	Secret(String),
	/// Base url for HTTP requests
	HttpUrl(MexcHttpUrl),
	/// Authentication type for HTTP requests
	HttpAuth(MexcAuth),
	/// RequestConfig used when sending requests
	RequestConfig(RequestConfig),
	/// Base url for WebSocket connections
	WebSocketUrl(MexcWebSocketUrl),
	/// WebSocketConfig used for creating WebSocketConnections
	WebSocketConfig(WebSocketConfig),
}

/// A struct that represents a set of MexcOptions
#[derive(Clone, derive_more::Debug)]
pub struct MexcOptions {
	/// see [MexcOption::Key]
	pub key: Option<String>,
	/// see [MexcOption::Secret]
	#[debug("[REDACTED]")]
	pub secret: Option<String>,
	/// see [MexcOption::HttpUrl]
	pub http_url: MexcHttpUrl,
	/// see [MexcOption::HttpAuth]
	pub http_auth: MexcAuth,
	/// see [MexcOption::RequestConfig]
	pub request_config: RequestConfig,
	/// see [MexcOption::WebSocketUrl]
	pub websocket_url: MexcWebSocketUrl,
	/// see [MexcOption::WebSocketConfig]
	pub websocket_config: WebSocketConfig,
}

/// Enum that represents the base url of the MEXC REST API
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[non_exhaustive]
pub enum MexcHttpUrl {
	/// Main API endpoint
	Spot,
	/// Testnet API endpoint
	SpotTest,
	/// The url will not be modified by MexcRequestHandler
	None,
}

/// Enum that represents the base url of the MEXC WebSocket API
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[non_exhaustive]
pub enum MexcWebSocketUrl {
	/// Main WebSocket endpoint
	Spot,
	/// Testnet WebSocket endpoint
	SpotTest,
	/// The url will not be modified by MexcWebSocketHandler
	None,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum MexcAuth {
	Sign,
	Key,
	None,
}

#[derive(Debug)]
pub enum MexcHandlerError {
	ApiError(MexcError),
	RateLimitError { retry_after: Option<u32> },
	ParseError,
}

#[derive(Deserialize, Debug)]
pub struct MexcError {
	pub code: i32,
	pub msg: String,
}

/// A struct that implements RequestHandler
pub struct MexcRequestHandler<'a, R: DeserializeOwned> {
	options: MexcOptions,
	_phantom: PhantomData<&'a R>,
}

/// A struct that implements WebSocketHandler
pub struct MexcWebSocketHandler {
	message_handler: Box<dyn FnMut(serde_json::Value) + Send>,
	options: MexcOptions,
}

impl<B, R> RequestHandler<B> for MexcRequestHandler<'_, R>
where
	B: Serialize,
	R: DeserializeOwned,
{
	type BuildError = &'static str;
	type Successful = R;
	type Unsuccessful = MexcHandlerError;

	fn request_config(&self) -> RequestConfig {
		let mut config = self.options.request_config.clone();
		if self.options.http_url != MexcHttpUrl::None {
			config.url_prefix = self.options.http_url.as_str().to_owned();
		}
		config
	}

	#[tracing::instrument(skip_all, fields(?builder))]
	fn build_request(&self, mut builder: RequestBuilder, request_body: &Option<B>, _: u8) -> Result<Request, Self::BuildError> {
		if let Some(body) = request_body {
			let encoded = serde_urlencoded::to_string(body).or(Err("could not serialize body as application/x-www-form-urlencoded"))?;
			builder = builder.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(encoded);
		}

		if self.options.http_auth != MexcAuth::None {
			let key = self.options.key.as_deref().ok_or("API key not set")?;
			builder = builder.header("X-MEXC-APIKEY", key);

			if self.options.http_auth == MexcAuth::Sign {
				let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
				let timestamp = time.as_millis();

				builder = builder.query(&[("timestamp", timestamp)]);

				let secret = self.options.secret.as_deref().ok_or("API secret not set")?;
				let mut hmac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();

				let mut request = builder.build().or(Err("Failed to build request"))?;
				let query = request.url().query().unwrap();
				let body = request.body().and_then(|body| body.as_bytes()).unwrap_or_default();

				hmac.update(&[query.as_bytes(), body].concat());
				let signature = hex::encode(hmac.finalize().into_bytes());

				request.url_mut().query_pairs_mut().append_pair("signature", &signature);

				return Ok(request);
			}
		}
		builder.build().or(Err("failed to build request"))
	}

	fn handle_response(&self, status: StatusCode, headers: HeaderMap, response_body: Bytes) -> Result<Self::Successful, Self::Unsuccessful> {
		if status.is_success() {
			serde_json::from_slice(&response_body).map_err(|error| {
				tracing::debug!("Failed to parse response due to an error: {}", error);
				MexcHandlerError::ParseError
			})
		} else {
			if status == 429 {
				let retry_after = if let Some(value) = headers.get("Retry-After") {
					if let Ok(string) = value.to_str() {
						if let Ok(retry_after) = u32::from_str(string) {
							Some(retry_after)
						} else {
							tracing::debug!("Invalid number in Retry-After header");
							None
						}
					} else {
						tracing::debug!("Non-ASCII character in Retry-After header");
						None
					}
				} else {
					None
				};
				return Err(MexcHandlerError::RateLimitError { retry_after });
			}

			let error = match serde_json::from_slice(&response_body) {
				Ok(parsed_error) => MexcHandlerError::ApiError(parsed_error),
				Err(error) => {
					tracing::debug!("Failed to parse error response due to an error: {}", error);
					MexcHandlerError::ParseError
				}
			};
			Err(error)
		}
	}
}

impl WebSocketHandler for MexcWebSocketHandler {
	fn websocket_config(&self) -> WebSocketConfig {
		let mut config = self.options.websocket_config.clone();
		if self.options.websocket_url != MexcWebSocketUrl::None {
			config.url_prefix = self.options.websocket_url.as_str().to_owned();
		}
		config
	}

	fn handle_message(&mut self, message: WebSocketMessage) -> Vec<WebSocketMessage> {
		match message {
			WebSocketMessage::Text(message) =>
				if let Ok(message) = serde_json::from_str(&message) {
					(self.message_handler)(message);
				} else {
					tracing::debug!("Invalid JSON message received");
				},
			WebSocketMessage::Binary(_) => tracing::debug!("Unexpected binary message received"),
			WebSocketMessage::Ping(_) | WebSocketMessage::Pong(_) => (),
		}
		vec![]
	}
}

impl MexcHttpUrl {
	/// The URL that this variant represents
	#[inline(always)]
	fn as_str(&self) -> &'static str {
		match self {
			Self::Spot => "https://api.mexc.com",
			Self::SpotTest => "https://api-testnet.mexc.com",
			Self::None => "",
		}
	}
}

impl MexcWebSocketUrl {
	/// The URL that this variant represents
	#[inline(always)]
	pub fn as_str(&self) -> &'static str {
		match self {
			Self::Spot => "wss://stream.mexc.com/ws",
			Self::SpotTest => "wss://stream-testnet.mexc.com/ws",
			Self::None => "",
		}
	}
}

impl HandlerOptions for MexcOptions {
	type OptionItem = MexcOption;

	fn update(&mut self, option: Self::OptionItem) {
		match option {
			MexcOption::Default => (),
			MexcOption::Key(v) => self.key = Some(v),
			MexcOption::Secret(v) => self.secret = Some(v),
			MexcOption::HttpUrl(v) => self.http_url = v,
			MexcOption::HttpAuth(v) => self.http_auth = v,
			MexcOption::RequestConfig(v) => self.request_config = v,
			MexcOption::WebSocketUrl(v) => self.websocket_url = v,
			MexcOption::WebSocketConfig(v) => self.websocket_config = v,
		}
	}
}

impl Default for MexcOptions {
	fn default() -> Self {
		let mut websocket_config = WebSocketConfig::new();
		websocket_config.refresh_after = Duration::from_secs(60 * 60 * 12);
		websocket_config.ignore_duplicate_during_reconnection = true;
		Self {
			key: None,
			secret: None,
			http_url: MexcHttpUrl::None,
			http_auth: MexcAuth::None,
			request_config: RequestConfig::default(),
			websocket_url: MexcWebSocketUrl::None,
			websocket_config,
		}
	}
}

impl<'a, R, B> HttpOption<'a, R, B> for MexcOption
where
	R: DeserializeOwned + 'a,
	B: Serialize,
{
	type RequestHandler = MexcRequestHandler<'a, R>;

	#[inline(always)]
	fn request_handler(options: Self::Options) -> Self::RequestHandler {
		MexcRequestHandler::<'a, R> { options, _phantom: PhantomData }
	}
}

impl<H: FnMut(serde_json::Value) + Send + 'static> WebSocketOption<H> for MexcOption {
	type WebSocketHandler = MexcWebSocketHandler;

	#[inline(always)]
	fn websocket_handler(handler: H, options: Self::Options) -> Self::WebSocketHandler {
		MexcWebSocketHandler {
			message_handler: Box::new(handler),
			options,
		}
	}
}

impl HandlerOption for MexcOption {
	type Options = MexcOptions;
}

impl Default for MexcOption {
	fn default() -> Self {
		Self::Default
	}
}
