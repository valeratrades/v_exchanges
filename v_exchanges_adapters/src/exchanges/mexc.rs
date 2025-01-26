// A module for communicating with the MEXC API (https://mexcdevelop.github.io/apidocs/spot/en/)

use std::{
	marker::PhantomData,
	str::FromStr,
	time::{self, SystemTime},
};

use chrono::{Duration, Utc};
use hmac::{Hmac, Mac};
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::Sha256;
use v_exchanges_api_generics::{http::*, websocket::*};
use v_utils::prelude::*;

use crate::traits::*;

static MAX_RECV_WINDOW: u16 = 60000; // as of (2025/01/18)

/// Options that can be set when creating handlers
pub enum MexcOption {
	/// [Default] variant, does nothing
	Default,
	/// API key
	Key(String),
	/// Api secret
	Secret(SecretString),
	/// Base url for HTTP requests
	HttpUrl(MexcHttpUrl),
	/// Authentication type for HTTP requests
	HttpAuth(MexcAuth),
	/// receive window parameter used for requests
	RecvWindow(u16),
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
	pub secret: Option<SecretString>,
	/// see [MexcOption::HttpUrl]
	pub http_url: MexcHttpUrl,
	/// see [MexcOption::HttpAuth]
	pub http_auth: MexcAuth,
	/// see [MexcOption::RecvWindow]
	pub recv_window: Option<u16>,
	/// see [MexcOption::RequestConfig]
	pub request_config: RequestConfig,
	/// see [MexcOption::WebSocketUrl]
	pub websocket_url: MexcWebSocketUrl,
	/// see [MexcOption::WebSocketConfig]
	pub websocket_config: WebSocketConfig,
}

/// Enum that represents the base url of the MEXC REST API
#[derive(Debug, Eq, PartialEq, Copy, Clone, Default)]
#[non_exhaustive]
pub enum MexcHttpUrl {
	Spot,
	SpotTest,
	Futures,
	#[default]
	None,
}
impl MexcHttpUrl {
	/// The URL that this variant represents
	#[inline(always)]
	fn as_str(&self) -> &'static str {
		match self {
			Self::Spot => "https://api.mexc.com",
			Self::SpotTest => "https://api-testnet.mexc.com",
			Self::Futures => "https://contract.mexc.com",
			Self::None => "",
		}
	}
}

/// Enum that represents the base url of the MEXC WebSocket API
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[non_exhaustive]
pub enum MexcWebSocketUrl {
	Spot,
	SpotTest,
	Futures,
	None,
}
impl MexcWebSocketUrl {
	/// The URL that this variant represents
	#[inline(always)]
	pub fn as_str(&self) -> &'static str {
		match self {
			Self::Spot => "wss://stream.mexc.com/ws",
			Self::SpotTest => "wss://stream-testnet.mexc.com/ws",
			Self::Futures => "wss://contract.mexc.com/ws",
			Self::None => "",
		}
	}
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum MexcAuth {
	Sign,
	Key,
	None,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct MexcError {
	pub code: i32,
	pub msg: String,
}
impl From<MexcError> for ApiError {
	fn from(e: MexcError) -> Self {
		ApiError::Other(eyre!("MEXC API error: {}: {}", e.code, e.msg))
	}
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
	type Successful = R;

	fn base_url(&self) -> String {
		self.options.http_url.as_str().to_owned()
	}

	#[tracing::instrument(skip_all, fields(?builder))]
	fn build_request(&self, mut builder: RequestBuilder, request_body: &Option<B>, _: u8) -> Result<Request, BuildError> {
		if let Some(body) = request_body {
			let encoded = serde_urlencoded::to_string(body)?;
			builder = builder.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(encoded);
			//builder = builder.header(header::CONTENT_TYPE, "application/json");
		}

		if self.options.http_auth != MexcAuth::None {
			let key = self.options.key.as_deref().ok_or(AuthError::MissingApiKey)?;
			builder = builder.header("ApiKey", key);

			let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
			let timestamp = time.as_millis();
			builder = builder.header("Request-Time", timestamp.to_string());

			if let Some(recv_window) = self.options.recv_window {
				builder = builder.header("Recv-Window", recv_window.to_string());
			}

			if self.options.http_auth == MexcAuth::Sign {
				let secret = self.options.secret.as_ref().map(|s| s.expose_secret()).ok_or(AuthError::MissingSecret)?;
				let mut hmac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();

				let mut request = builder.build().expect("My understanding is that this doesn't fail on client, so fail fast for dev");
				let param_string = if request.method() == Method::GET || request.method() == Method::DELETE {
					if let Some(body) = request_body { serde_urlencoded::to_string(body)? } else { String::new() }
				} else {
					// For POST, use body as JSON string
					String::from_utf8(request.body().and_then(|body| body.as_bytes()).unwrap_or_default().to_vec()).unwrap_or_default()
				};

				let signature_base = format!("{}{}{}", key, timestamp, param_string);
				hmac.update(signature_base.as_bytes());
				let signature = hex::encode(hmac.finalize().into_bytes());
				request.headers_mut().insert("Signature", signature.parse().unwrap());

				return Ok(request);
			}
		}
		Ok(builder.build().expect("Don't expect this to be reached by client. Same reasoning - fail fast for dev"))
	}

	fn handle_response(&self, status: StatusCode, headers: HeaderMap, response_body: Bytes) -> Result<Self::Successful, HandleError> {
		if status.is_success() {
			serde_json::from_slice(&response_body).map_err(|e| {
				tracing::debug!("Failed to parse response due to an error: {e}",);
				HandleError::Parse(e)
			})
		} else {
			//Q: does MEXC even have this, or am I just blindly copying from Binance?
			if status == 429 {
				let retry_after_sec = if let Some(value) = headers.get("Retry-After") {
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
				let e = match retry_after_sec {
					Some(s) => {
						let until = Some(Utc::now() + Duration::seconds(s as i64));
						ApiError::IpTimeout { until }.into()
					}
					None => eyre!("Could't interpret Retry-After header").into(),
				};
				return Err(e);
			}

			let api_error: MexcError = match serde_json::from_slice(&response_body) {
				Ok(parsed) => parsed,
				Err(e) => return Err(HandleError::Parse(e)),
			};
			Err(ApiError::from(api_error).into())
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

impl HandlerOptions for MexcOptions {
	type OptionItem = MexcOption;

	fn update(&mut self, option: Self::OptionItem) {
		match option {
			MexcOption::Default => (),
			MexcOption::Key(v) => self.key = Some(v),
			MexcOption::Secret(v) => self.secret = Some(v),
			MexcOption::HttpUrl(v) => self.http_url = v,
			MexcOption::HttpAuth(v) => self.http_auth = v,
			MexcOption::RecvWindow(v) =>
				if v > MAX_RECV_WINDOW {
					tracing::warn!("recvWindow is too large, overwriting with maximum value of {MAX_RECV_WINDOW}");
					self.recv_window = Some(MAX_RECV_WINDOW);
				} else {
					self.recv_window = Some(v);
				},
			MexcOption::RequestConfig(v) => self.request_config = v,
			MexcOption::WebSocketUrl(v) => self.websocket_url = v,
			MexcOption::WebSocketConfig(v) => self.websocket_config = v,
		}
	}

	fn is_authenticated(&self) -> bool {
		self.key.is_some() // some end points are satisfied with just the key, and it's really difficult to provide only a key without a secret from the clientside, so assume intent if it's missing.
	}
}

impl Default for MexcOptions {
	fn default() -> Self {
		let mut websocket_config = WebSocketConfig::new();
		websocket_config.refresh_after = time::Duration::from_secs(60 * 60 * 12);
		websocket_config.ignore_duplicate_during_reconnection = true;
		Self {
			key: None,
			secret: None,
			http_url: MexcHttpUrl::None,
			http_auth: MexcAuth::None,
			recv_window: None,
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
