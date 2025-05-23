//! A module for communicating with the [coincheck API](https://coincheck.com/ja/documents/exchange/api).
//! For example usages, see files in the examples/ directory.

use std::{marker::PhantomData, time::SystemTime};

use hmac::{Hmac, Mac};
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::json;
use sha2::Sha256;
use v_exchanges_api_generics::{
	http::{header::HeaderValue, *},
	websocket::*,
};

use crate::traits::*;

/// The type returned by [Client::request()].
pub type CoincheckRequestResult<T> = Result<T, CoincheckRequestError>;
pub type CoincheckRequestError = RequestError<&'static str, CoincheckHandlerError>;

/// Options that can be set when creating handlers
#[derive(Debug, Default)]
pub enum CoincheckOption {
	/// [Default] variant, does nothing
	#[default]
	Default,
	/// API key
	Key(String),
	/// Api secret
	Secret(SecretString),
	/// Base url for HTTP requests
	HttpUrl(CoincheckHttpUrl),
	/// Whether [CoincheckRequestHandler] should perform authentication
	HttpAuth(bool),
	/// [RequestConfig] used when sending requests.
	/// `url_prefix` will be overridden by [HttpUrl](Self::HttpUrl) unless `HttpUrl` is [CoincheckHttpUrl::None].
	RequestConfig(RequestConfig),
	/// Base url for WebSocket connections
	WebSocketUrl(CoincheckWebSocketUrl),
	/// The channels to be subscribed by [WebSocketHandler].
	WebSocketChannels(Vec<String>),
	/// [WebSocketConfig] used for creating [WebSocketConnection]s
	/// `url_prefix` will be overridden by [WebSocketUrl](Self::WebSocketUrl) unless `WebSocketUrl` is [CoincheckWebSocketUrl::None].
	/// By default, ignore_duplicate_during_reconnection` is set to `true`.
	WebSocketConfig(WebSocketConfig),
}

/// A `struct` that represents a set of [CoincheckOption] s.
#[derive(Clone, derive_more::Debug)]
pub struct CoincheckOptions {
	/// see [CoincheckOption::Key]
	pub key: Option<String>,
	/// see [CoincheckOption::Secret]
	#[debug("[REDACTED]")]
	pub secret: Option<SecretString>,
	/// see [CoincheckOption::HttpUrl]
	pub http_url: CoincheckHttpUrl,
	/// see [CoincheckOption::HttpAuth]
	pub http_auth: bool,
	/// see [CoincheckOption::RequestConfig]
	pub request_config: RequestConfig,
	/// see [CoincheckOption::WebSocketUrl]
	pub websocket_url: CoincheckWebSocketUrl,
	/// see [CoincheckOption::WebSocketChannels]
	pub websocket_channels: Vec<String>,
	/// see [CoincheckOption::WebSocketConfig]
	pub websocket_config: WebSocketConfig,
}

/// A `enum` that represents the base url of the Coincheck HTTP API.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CoincheckHttpUrl {
	/// `https://coincheck.com`
	Main,
	/// The url will not be modified by [CoincheckRequestHandler]
	#[default]
	None,
}

/// A `enum` that represents the base url of the Coincheck Realtime API
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CoincheckWebSocketUrl {
	/// `wss://ws-api.coincheck.com/`
	Default,
	/// The url will not be modified by [CoincheckWebSocketHandler]
	None,
}

#[derive(Debug)]
pub enum CoincheckHandlerError {
	ApiError(serde_json::Value),
	RequestLimitExceeded(serde_json::Value),
	ParseError,
}

/// A `struct` that implements [RequestHandler]
pub struct CoincheckRequestHandler<'a, R: DeserializeOwned> {
	options: CoincheckOptions,
	_phantom: PhantomData<&'a R>,
}

/// A `struct` that implements [WebSocketHandler]
pub struct CoincheckWebSocketHandler {
	message_handler: Box<dyn FnMut(serde_json::Value) + Send>,
	options: CoincheckOptions,
}

impl<B, R> RequestHandler<B> for CoincheckRequestHandler<'_, R>
where
	B: Serialize,
	R: DeserializeOwned,
{
	type BuildError = &'static str;
	type Successful = R;
	type Unsuccessful = CoincheckHandlerError;

	fn base_url(&self, is_test: bool) -> String {
		match is_test {
			true => todo!(),
			false => self.options.http_url.as_str().to_owned(),
		}
	}

	fn build_request(&self, mut builder: RequestBuilder, request_body: &Option<B>, _: u8) -> Result<Request, Self::BuildError> {
		if let Some(body) = request_body {
			let encoded = serde_urlencoded::to_string(body).or(Err("could not serialize body as application/x-www-form-urlencoded"))?;
			builder = builder.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(encoded);
		}

		let mut request = builder.build().or(Err("failed to build request"))?;

		if self.options.http_auth {
			// https://coincheck.com/ja/documents/exchange/api#auth
			let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap(); // always after the epoch
			let timestamp = time.as_millis() as u64;

			let body = request.body().and_then(|body| body.as_bytes()).map(String::from_utf8_lossy).unwrap_or_default();

			let sign_contents = format!("{}{}{}", timestamp, request.url(), body);

			let secret = self.options.secret.as_ref().map(|s| s.expose_secret()).ok_or("API secret not set")?;
			let mut hmac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap(); // hmac accepts key of any length

			hmac.update(sign_contents.as_bytes());
			let signature = hex::encode(hmac.finalize().into_bytes());

			let key = HeaderValue::from_str(self.options.key.as_deref().ok_or("API key not set")?).or(Err("invalid character in API key"))?;
			let headers = request.headers_mut();
			headers.insert("ACCESS-KEY", key);
			headers.insert("ACCESS-NONCE", HeaderValue::from(timestamp));
			headers.insert("ACCESS-SIGNATURE", HeaderValue::from_str(&signature).unwrap()); // hex digits are valid
		}

		Ok(request)
	}

	fn handle_response(&self, status: StatusCode, _: HeaderMap, response_body: Bytes) -> Result<Self::Successful, Self::Unsuccessful> {
		if status.is_success() {
			serde_json::from_slice(&response_body).map_err(|error| {
				tracing::debug!("Failed to parse response due to an error: {}", error);
				CoincheckHandlerError::ParseError
			})
		} else {
			let error = match serde_json::from_slice(&response_body) {
				Ok(parsed_error) =>
					if status == 429 {
						CoincheckHandlerError::RequestLimitExceeded(parsed_error)
					} else {
						CoincheckHandlerError::ApiError(parsed_error)
					},
				Err(error) => {
					tracing::debug!("Failed to parse error response due to an error: {}", error);
					CoincheckHandlerError::ParseError
				}
			};
			Err(error)
		}
	}
}

impl WebSocketHandler for CoincheckWebSocketHandler {
	fn websocket_config(&self) -> WebSocketConfig {
		let mut config = self.options.websocket_config.clone();
		if self.options.websocket_url != CoincheckWebSocketUrl::None {
			config.url_prefix = self.options.websocket_url.as_str().to_owned();
		}
		config
	}

	fn handle_start(&mut self) -> Vec<WebSocketMessage> {
		self.options
			.websocket_channels
			.clone()
			.into_iter()
			.map(|channel| WebSocketMessage::Text(json!({ "type": "subscribe", "channel": channel }).to_string()))
			.collect()
	}

	fn handle_message(&mut self, message: WebSocketMessage) -> Vec<WebSocketMessage> {
		match message {
			WebSocketMessage::Text(message) => {
				match serde_json::from_str(&message) {
					Ok(message) => (self.message_handler)(message),
					Err(_) => tracing::debug!("Invalid JSON message received"),
				};
			}
			WebSocketMessage::Binary(_) => tracing::debug!("Unexpected binary message received"),
			WebSocketMessage::Ping(_) | WebSocketMessage::Pong(_) => (),
		}
		vec![]
	}
}

impl CoincheckHttpUrl {
	/// The base URL that this variant represents.
	fn as_str(&self) -> &'static str {
		match self {
			Self::Main => "https://coincheck.com",
			Self::None => "",
		}
	}
}

impl CoincheckWebSocketUrl {
	/// The base URL that this variant represents.
	fn as_str(&self) -> &'static str {
		match self {
			Self::Default => "wss://ws-api.coincheck.com/",
			Self::None => "",
		}
	}
}

impl HandlerOptions for CoincheckOptions {
	type OptionItem = CoincheckOption;

	fn update(&mut self, option: Self::OptionItem) {
		match option {
			CoincheckOption::Default => (),
			CoincheckOption::Key(v) => self.key = Some(v),
			CoincheckOption::Secret(v) => self.secret = Some(v),
			CoincheckOption::HttpUrl(v) => self.http_url = v,
			CoincheckOption::HttpAuth(v) => self.http_auth = v,
			CoincheckOption::RequestConfig(v) => self.request_config = v,
			CoincheckOption::WebSocketUrl(v) => self.websocket_url = v,
			CoincheckOption::WebSocketChannels(v) => self.websocket_channels = v,
			CoincheckOption::WebSocketConfig(v) => self.websocket_config = v,
		}
	}

	fn is_authenticated(&self) -> bool {
		self.key.is_some() // some endpoints are satisfied with just the key, and it's really difficult to provide only a key without a secret from the clientside, so assume intent if it's missing.
	}
}

impl Default for CoincheckOptions {
	fn default() -> Self {
		let mut websocket_config = WebSocketConfig::default();
		websocket_config.ignore_duplicate_during_reconnection = true;
		Self {
			key: None,
			secret: None,
			http_url: CoincheckHttpUrl::Main,
			http_auth: false,
			request_config: RequestConfig::default(),
			websocket_url: CoincheckWebSocketUrl::Default,
			websocket_channels: vec![],
			websocket_config,
		}
	}
}

impl<'a, R, B> HttpOption<'a, R, B> for CoincheckOption
where
	R: DeserializeOwned + 'a,
	B: Serialize,
{
	type RequestHandler = CoincheckRequestHandler<'a, R>;

	fn request_handler(options: Self::Options) -> Self::RequestHandler {
		CoincheckRequestHandler::<'a, R> { options, _phantom: PhantomData }
	}
}

impl<H: FnMut(serde_json::Value) + Send + 'static> WebSocketOption<H> for CoincheckOption {
	type WebSocketHandler = CoincheckWebSocketHandler;

	fn websocket_handler(handler: H, options: Self::Options) -> Self::WebSocketHandler {
		CoincheckWebSocketHandler {
			message_handler: Box::new(handler),
			options,
		}
	}
}

impl HandlerOption for CoincheckOption {
	type Options = CoincheckOptions;
}
