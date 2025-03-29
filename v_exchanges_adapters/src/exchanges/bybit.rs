//! A module for communicating with the [Bybit API](https://bybit-exchange.github.io/docs/spot/v3/#t-introduction).
//! For example usages, see files in the examples/ directory.

use std::{borrow::Cow, marker::PhantomData, time::SystemTime, vec};

use hmac::{Hmac, Mac};
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::json;
use sha2::Sha256;
use v_exchanges_api_generics::{
	http::{header::HeaderValue, *},
	websocket::*,
};
use v_utils::prelude::*;

use crate::traits::*;

/// Options that can be set when creating handlers
#[derive(Debug, Default)]
pub enum BybitOption {
	#[default]
	None,
	/// API key
	Pubkey(String),
	/// Api secret
	Secret(SecretString),
	/// Base url for HTTP requests
	HttpUrl(BybitHttpUrl),
	/// Type of authentication used for HTTP requests.
	HttpAuth(BybitHttpAuth),
	/// receive window parameter used for requests
	RecvWindow(u16),
	/// Base url for WebSocket connections
	WebSocketUrl(BybitWebSocketUrl),
	/// Whether [BybitWebSocketHandler] should perform authentication
	WebSocketAuth(bool),
	/// The topics to subscribe to.
	WebSocketTopics(Vec<String>),
	/// [WebSocketConfig] used for creating [WebSocketConnection]s
	/// `url_prefix` will be overridden by [WebSocketUrl](Self::WebSocketUrl) unless `WebSocketUrl` is [BybitWebSocketUrl::None].
	/// By default, `ignore_duplicate_during_reconnection` is set to `true`.
	WebSocketConfig(WebSocketConfig),
}

/// A `struct` that represents a set of [BybitOption] s.
#[derive(Clone, derive_more::Debug)]
pub struct BybitOptions {
	/// see [BybitOption::Key]
	pub pubkey: Option<String>,
	/// see [BybitOption::Secret]
	#[debug("[REDACTED]")]
	pub secret: Option<SecretString>,
	/// see [BybitOption::HttpUrl]
	pub http_url: BybitHttpUrl,
	/// see [BybitOption::HttpAuth]
	pub http_auth: BybitHttpAuth,
	/// see [BybitOption::RecvWindow]
	pub recv_window: Option<u16>,
	/// see [BybitOption::WebSocketUrl]
	pub websocket_url: BybitWebSocketUrl,
	/// see [BybitOption::WebSocketAuth]
	pub websocket_auth: bool,
	/// see [BybitOption::WebSocketTopics]
	pub websocket_topics: Vec<String>,
	/// see [BybitOption::WebSocketConfig]
	pub websocket_config: WebSocketConfig,
}

/// A `enum` that represents the base url of the Bybit REST API.
#[derive(Debug, Eq, PartialEq, Copy, Clone, Default)]
pub enum BybitHttpUrl {
	/// `https://api.bybit.com`
	#[default]
	Bybit,
	/// `https://api.bytick.com`
	Bytick,
	/// `https://api-testnet.bybit.com`
	Test,
	/// The url will not be modified by [BybitRequestHandler]
	None,
}

/// A `enum` that represents the base url of the Bybit WebSocket API.
#[derive(Debug, Eq, PartialEq, Copy, Clone, Default)]
pub enum BybitWebSocketUrl {
	/// `wss://stream.bybit.com`
	#[default]
	Bybit,
	/// `wss://stream.bytick.com`
	Bytick,
	/// `wss://stream-testnet.bybit.com`
	Test,
	/// The url will not be modified by [BybitWebSocketHandler]
	None,
}

/// Represents the auth type.
#[derive(Debug, Eq, PartialEq, Copy, Clone, Default)]
pub enum BybitHttpAuth {
	/// [Spot V1](https://bybit-exchange.github.io/docs-legacy/spot/v1/#t-introduction)
	SpotV1,
	/// "Previous Version" APIs except for [Spot V1](https://bybit-exchange.github.io/docs-legacy/spot/v1/#t-introduction),
	/// [USDC Option](https://bybit-exchange.github.io/docs-legacy/usdc/option/#t-introduction), and
	/// [USDC Perpetual](https://bybit-exchange.github.io/docs-legacy/usdc/perpetual/#t-introduction)
	BelowV3,
	/// [USDC Option](https://bybit-exchange.github.io/docs-legacy/usdc/option/#t-introduction) and
	/// [USDC Perpetual](https://bybit-exchange.github.io/docs-legacy/usdc/perpetual/#t-introduction)
	UsdcContractV1,
	/// [V3](https://bybit-exchange.github.io/docs/v3/intro) and [V5](https://bybit-exchange.github.io/docs/v5/intro)
	V3AndAbove,
	/// No authentication (for public APIs)
	#[default]
	None,
}

#[derive(Debug)]
pub enum BybitHandlerError {
	ApiError(serde_json::Value),
	IpBan(serde_json::Value),
	ParseError,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct BybitError {
	code: i16,
	msg: String,
}
impl From<BybitError> for ApiError {
	fn from(e: BybitError) -> Self {
		//HACK
		ApiError::Other(eyre!("Bybit error {}: {}", e.code, e.msg))
	}
}

/// A `struct` that implements [RequestHandler]
pub struct BybitRequestHandler<'a, R: DeserializeOwned> {
	options: BybitOptions,
	_phantom: PhantomData<&'a R>,
}

pub struct BybitWebSocketHandler {
	message_handler: Box<dyn FnMut(serde_json::Value) + Send>,
	options: BybitOptions,
}

impl<B, R> RequestHandler<B> for BybitRequestHandler<'_, R>
where
	B: Serialize,
	R: DeserializeOwned,
{
	type Successful = R;

	fn base_url(&self, is_test: bool) -> String {
		match is_test {
			true => todo!(),
			false => self.options.http_url.as_str().to_owned(),
		}
	}

	fn build_request(&self, mut builder: RequestBuilder, request_body: &Option<B>, _: u8) -> Result<Request, BuildError> {
		if self.options.http_auth == BybitHttpAuth::None {
			if let Some(body) = request_body {
				let json = serde_json::to_string(body)?;
				builder = builder.header(header::CONTENT_TYPE, "application/json").body(json);
			}
			return Ok(builder.build().expect("My understanding is client can't trigger this. So fail fast for dev"));
		}

		let pubkey = self.options.pubkey.as_deref().ok_or(AuthError::MissingApiKey)?;
		let secret = self.options.secret.as_ref().map(|s| s.expose_secret()).ok_or(AuthError::MissingSecret)?;

		let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap(); // always after the epoch
		let timestamp = time.as_millis();

		let hmac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap(); // hmac accepts key of any length

		match self.options.http_auth {
			BybitHttpAuth::SpotV1 => Self::v1_auth(builder, request_body, pubkey, timestamp, hmac, true, self.options.recv_window),
			BybitHttpAuth::BelowV3 => Self::v1_auth(builder, request_body, pubkey, timestamp, hmac, false, self.options.recv_window),
			BybitHttpAuth::UsdcContractV1 => Self::v3_auth(builder, request_body, pubkey, timestamp, hmac, true, self.options.recv_window),
			BybitHttpAuth::V3AndAbove => Self::v3_auth(builder, request_body, pubkey, timestamp, hmac, false, self.options.recv_window),
			BybitHttpAuth::None => unreachable!(), // we've already handled this case
		}
	}

	fn handle_response(&self, status: StatusCode, _: HeaderMap, response_body: Bytes) -> Result<Self::Successful, HandleError> {
		if status.is_success() {
			serde_json::from_slice(&response_body).map_err(|error| {
				tracing::debug!("Failed to parse response due to an error: {}", error);
				HandleError::Parse(error)
			})
		} else {
			// https://bybit-exchange.github.io/docs/spot/v3/#t-ratelimits
			let api_error: BybitError = match serde_json::from_slice(&response_body) {
				Ok(parsed) =>
					if status == 403 {
						return Err(ApiError::IpTimeout { until: None }.into());
					} else {
						parsed
					},
				Err(e) => {
					tracing::debug!("Failed to parse error response due to an error: {e}");
					return Err(HandleError::Parse(e));
				}
			};
			Err(ApiError::from(api_error).into())
		}
	}
}

impl<R> BybitRequestHandler<'_, R>
where
	R: DeserializeOwned,
{
	fn v1_auth<B>(builder: RequestBuilder, request_body: &Option<B>, key: &str, timestamp: u128, mut hmac: Hmac<Sha256>, spot: bool, window: Option<u16>) -> Result<Request, BuildError>
	where
		B: Serialize, {
		fn sort_and_add<'a>(mut pairs: Vec<(Cow<str>, Cow<'a, str>)>, key: &'a str, timestamp: u128) -> String {
			pairs.push((Cow::Borrowed("api_key"), Cow::Borrowed(key)));
			pairs.push((Cow::Borrowed("timestamp"), Cow::Owned(timestamp.to_string())));
			pairs.sort_unstable();

			let mut urlencoded = String::new();
			for (key, value) in pairs {
				urlencoded.push_str(&key);
				if !value.is_empty() {
					urlencoded.push('=');
					urlencoded.push_str(&value);
				}
				urlencoded.push('&');
			}
			urlencoded.pop(); // the last '&'
			urlencoded
		}

		let mut request = builder.build().expect("My understanding is client can't trigger this. So fail fast for dev");
		if matches!(*request.method(), Method::GET | Method::DELETE) {
			let mut queries: Vec<_> = request.url().query_pairs().collect();
			if let Some(window) = window {
				if spot {
					queries.push((Cow::Borrowed("recvWindow"), Cow::Owned(window.to_string())));
				} else {
					queries.push((Cow::Borrowed("recv_window"), Cow::Owned(window.to_string())));
				}
			}
			let query = sort_and_add(queries, key, timestamp);
			request.url_mut().set_query(Some(&query));

			hmac.update(query.as_bytes());
			let signature = hex::encode(hmac.finalize().into_bytes());

			request.url_mut().query_pairs_mut().append_pair("sign", &signature);

			if let Some(body) = request_body {
				if spot {
					let body_string = serde_urlencoded::to_string(body)?;
					*request.body_mut() = Some(body_string.into());
					request.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/x-www-form-urlencoded"));
				} else {
					let body_string = serde_json::to_string(body)?;
					*request.body_mut() = Some(body_string.into());
					request.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
				}
			}
		} else {
			let mut body = if let Some(body) = request_body { serde_urlencoded::to_string(body)? } else { String::new() };
			if let Some(window) = window {
				if !body.is_empty() {
					body.push('&');
				}
				if spot {
					body.push_str("recvWindow=");
				} else {
					body.push_str("recv_window=");
				}
				body.push_str(&window.to_string());
			}

			let pairs: Vec<_> = body
				.split('&')
				.map(|pair| pair.split_once('=').unwrap_or((pair, "")))
				.map(|(k, v)| (Cow::Borrowed(k), Cow::Borrowed(v)))
				.collect();
			let mut sorted_query_string = sort_and_add(pairs, key, timestamp);

			hmac.update(sorted_query_string.as_bytes());
			let signature = hex::encode(hmac.finalize().into_bytes());

			sorted_query_string.push_str(&format!("&sign={signature}"));

			if spot {
				*request.body_mut() = Some(sorted_query_string.into());
				request.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/x-www-form-urlencoded"));
			} else {
				let body: serde_json::Value = serde_urlencoded::from_str(&sorted_query_string).unwrap(); // sorted_query_string is always in urlencoded format
				*request.body_mut() = Some(body.to_string().into());
				request.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
			}
		}
		Ok(request)
	}

	fn v3_auth<B>(
		mut builder: RequestBuilder,
		request_body: &Option<B>,
		key: &str,
		timestamp: u128,
		mut hmac: Hmac<Sha256>,
		version_header: bool,
		window: Option<u16>,
	) -> Result<Request, BuildError>
	where
		B: Serialize, {
		let body = if let Some(body) = request_body {
			let json = serde_json::to_value(body)?;
			builder = builder.header(header::CONTENT_TYPE, "application/json").body(json.to_string());
			Some(json)
		} else {
			None
		};

		let mut request = builder.build().expect("My understanding is client can't trigger this. So fail fast for dev");

		let mut sign_contents = format!("{timestamp}{key}");
		if let Some(window) = window {
			sign_contents.push_str(&window.to_string());
		}

		if matches!(*request.method(), Method::GET | Method::DELETE) {
			if let Some(query) = request.url().query() {
				sign_contents.push_str(query);
			}
		} else {
			let body = body.unwrap_or_else(|| {
				*request.body_mut() = Some("{}".into());
				request.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
				json!({})
			});
			sign_contents.push_str(&body.to_string());
		}

		hmac.update(sign_contents.as_bytes());
		let signature = hex::encode(hmac.finalize().into_bytes());

		let headers = request.headers_mut();
		if version_header {
			headers.insert("X-BAPI-SIGN-TYPE", HeaderValue::from(2));
		}
		headers.insert("X-BAPI-SIGN", HeaderValue::from_str(&signature).unwrap()); // hex digits are valid
		headers.insert("X-BAPI-API-KEY", HeaderValue::from_str(key).or(Err(AuthError::InvalidCharacterInApiKey(key.to_owned())))?);
		headers.insert("X-BAPI-TIMESTAMP", HeaderValue::from(timestamp as u64));
		if let Some(window) = window {
			headers.insert("X-BAPI-RECV-WINDOW", HeaderValue::from(window));
		}
		Ok(request)
	}
}

impl WebSocketHandler for BybitWebSocketHandler {
	fn websocket_config(&self) -> WebSocketConfig {
		let mut config = self.options.websocket_config.clone();
		if self.options.websocket_url != BybitWebSocketUrl::None {
			config.url_prefix = self.options.websocket_url.as_str().to_owned();
		}
		config
	}

	fn handle_start(&mut self) -> Vec<WebSocketMessage> {
		if self.options.websocket_auth {
			if let Some(pubkey) = self.options.pubkey.as_deref() {
				if let Some(secret) = self.options.secret.as_ref().map(|s| s.expose_secret()) {
					let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap(); // always after the epoch
					let expires = time.as_millis() as u64 + 1000;

					let mut hmac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap(); // hmac accepts key of any length

					hmac.update(format!("GET/realtime{expires}").as_bytes());
					let signature = hex::encode(hmac.finalize().into_bytes());

					return vec![WebSocketMessage::Text(
						json!({
							"op": "auth",
							"args": [pubkey, expires, signature],
						})
						.to_string(),
					)];
				} else {
					//Q: why does this not panic?
					tracing::error!("API secret not set.");
				};
			} else {
				tracing::error!("API pubkey not set.");
			};
		}
		self.message_subscribe()
	}

	fn handle_message(&mut self, message: WebSocketMessage) -> Vec<WebSocketMessage> {
		match message {
			WebSocketMessage::Text(message) => {
				let message: serde_json::Value = match serde_json::from_str(&message) {
					Ok(message) => message,
					Err(_) => {
						tracing::error!("Invalid JSON received");
						return vec![];
					}
				};
				match message["op"].as_str() {
					Some("auth") => {
						if message["success"].as_bool() == Some(true) {
							tracing::debug!("WebSocket authentication successful");
						} else {
							tracing::debug!("WebSocket authentication unsuccessful; message: {}", message["ret_msg"]);
						}
						return self.message_subscribe();
					}
					Some("subscribe") =>
						if message["success"].as_bool() == Some(true) {
							tracing::debug!("WebSocket topics subscription successful");
						} else {
							tracing::debug!("WebSocket topics subscription unsuccessful; message: {}", message["ret_msg"]);
						},
					_ => (self.message_handler)(message),
				}
			}
			WebSocketMessage::Binary(_) => tracing::debug!("Unexpected binary message received"),
			WebSocketMessage::Ping(_) | WebSocketMessage::Pong(_) => (),
		}
		vec![]
	}
}

impl BybitWebSocketHandler {
	#[inline(always)]
	fn message_subscribe(&self) -> Vec<WebSocketMessage> {
		vec![WebSocketMessage::Text(json!({ "op": "subscribe", "args": self.options.websocket_topics }).to_string())]
	}
}

impl BybitHttpUrl {
	/// The URL that this variant represents.
	#[inline(always)]
	pub fn as_str(&self) -> &'static str {
		match self {
			Self::Bybit => "https://api.bybit.com",
			Self::Bytick => "https://api.bytick.com",
			Self::Test => "https://api-testnet.bybit.com",
			Self::None => "",
		}
	}
}

impl BybitWebSocketUrl {
	/// The URL that this variant represents.
	#[inline(always)]
	pub fn as_str(&self) -> &'static str {
		match self {
			Self::Bybit => "wss://stream.bybit.com",
			Self::Bytick => "wss://stream.bytick.com",
			Self::Test => "wss://stream-testnet.bybit.com",
			Self::None => "",
		}
	}
}

impl Default for BybitOptions {
	fn default() -> Self {
		let mut websocket_config = WebSocketConfig::default();
		websocket_config.ignore_duplicate_during_reconnection = true;

		Self {
			websocket_config,
			pubkey: None,
			secret: None,
			http_url: BybitHttpUrl::default(),
			http_auth: BybitHttpAuth::default(),
			recv_window: None,
			websocket_url: BybitWebSocketUrl::default(),
			websocket_auth: false,
			websocket_topics: Vec::new(),
		}
	}
}

impl HandlerOptions for BybitOptions {
	type OptionItem = BybitOption;

	fn update(&mut self, option: Self::OptionItem) {
		match option {
			BybitOption::None => (),
			BybitOption::Pubkey(v) => self.pubkey = Some(v),
			BybitOption::Secret(v) => self.secret = Some(v),
			BybitOption::HttpUrl(v) => self.http_url = v,
			BybitOption::HttpAuth(v) => self.http_auth = v,
			BybitOption::RecvWindow(v) => self.recv_window = Some(v),
			BybitOption::WebSocketUrl(v) => self.websocket_url = v,
			BybitOption::WebSocketAuth(v) => self.websocket_auth = v,
			BybitOption::WebSocketTopics(v) => self.websocket_topics = v,
			BybitOption::WebSocketConfig(v) => self.websocket_config = v,
		}
	}

	fn is_authenticated(&self) -> bool {
		self.pubkey.is_some() // some endpoints are satisfied with just the key, and it's really difficult to provide only a key without a secret from the clientside, so assume intent if it's missing.
	}
}

impl<'a, R, B> HttpOption<'a, R, B> for BybitOption
where
	R: DeserializeOwned + 'a,
	B: Serialize,
{
	type RequestHandler = BybitRequestHandler<'a, R>;

	#[inline(always)]
	fn request_handler(options: Self::Options) -> Self::RequestHandler {
		BybitRequestHandler::<'a, R> { options, _phantom: PhantomData }
	}
}

impl<H: FnMut(serde_json::Value) + Send + 'static> WebSocketOption<H> for BybitOption {
	type WebSocketHandler = BybitWebSocketHandler;

	#[inline(always)]
	fn websocket_handler(handler: H, options: Self::Options) -> Self::WebSocketHandler {
		BybitWebSocketHandler {
			message_handler: Box::new(handler),
			options,
		}
	}
}

impl HandlerOption for BybitOption {
	type Options = BybitOptions;
}
