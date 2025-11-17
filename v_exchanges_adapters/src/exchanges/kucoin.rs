//! A module for communicating with the [Kucoin API](https://www.kucoin.com/docs/beginners/introduction).

use std::{collections::HashSet, marker::PhantomData, time::SystemTime};

use eyre::eyre;
use generics::{
	AuthError, UrlError,
	http::{ApiError, BuildError, HandleError, *},
	tokio_tungstenite::tungstenite,
	ws::{ContentEvent, ResponseOrContent, Topic, WsConfig, WsError, WsHandler},
};
use hmac::{Hmac, Mac};
use jiff::Timestamp;
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::Sha256;
use url::Url;
use v_utils::utils::truncate_msg;

use crate::traits::*;

// https://www.kucoin.com/docs/rest/account/basic-info/get-account-list-spot-margin-trade_hf
impl<B, R> RequestHandler<B> for KucoinRequestHandler<'_, R>
where
	B: Serialize,
	R: DeserializeOwned,
{
	type Successful = R;

	fn base_url(&self, is_test: bool) -> Result<Url, UrlError> {
		match is_test {
			true => self.options.http_url.url_testnet().ok_or_else(|| UrlError::MissingTestnet(self.options.http_url.url_mainnet())),
			false => Ok(self.options.http_url.url_mainnet()),
		}
	}

	#[tracing::instrument(skip_all, fields(?builder))]
	fn build_request(&self, mut builder: RequestBuilder, request_body: &Option<B>, _: u8) -> Result<Request, BuildError> {
		let body_str = if let Some(body) = request_body {
			let json = serde_json::to_string(body)?;
			builder = builder.header(header::CONTENT_TYPE, "application/json").body(json.clone());
			json
		} else {
			String::new()
		};

		if self.options.http_auth != KucoinAuth::None {
			let pubkey = self.options.pubkey.as_deref().ok_or(AuthError::MissingPubkey)?;
			let secret = self.options.secret.as_ref().map(|s| s.expose_secret()).ok_or(AuthError::MissingSecret)?;
			let passphrase = self
				.options
				.passphrase
				.as_ref()
				.map(|s| s.expose_secret())
				.ok_or_else(|| AuthError::Other(eyre!("Missing passphrase")))?;

			let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap(); // always after the epoch
			let timestamp = time.as_millis();

			let mut request = builder.build().expect("From what I understand, can't trigger this from client-side");

			// Build prehash string: timestamp + method + endpoint + body
			let method = request.method().as_str().to_string();
			let endpoint = request.url().path().to_string();
			let query = request.url().query().unwrap_or("").to_string();
			let endpoint_with_query = if query.is_empty() { endpoint.clone() } else { format!("{}?{}", endpoint, query) };

			let prehash = format!("{}{}{}{}", timestamp, method, endpoint_with_query, body_str);

			// Sign the prehash string
			let mut hmac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap(); // hmac accepts key of any length
			hmac.update(prehash.as_bytes());
			let signature = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, hmac.finalize().into_bytes());

			// Sign the passphrase
			let mut passphrase_hmac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
			passphrase_hmac.update(passphrase.as_bytes());
			let encrypted_passphrase = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, passphrase_hmac.finalize().into_bytes());

			// Add headers
			let headers = request.headers_mut();

			headers.insert(
				"KC-API-KEY",
				header::HeaderValue::from_str(pubkey).map_err(|e| AuthError::InvalidCharacterInApiKey(e.to_string()))?,
			);
			headers.insert(
				"KC-API-SIGN",
				header::HeaderValue::from_str(&signature).map_err(|e| AuthError::Other(eyre!("Invalid signature: {}", e)))?,
			);
			headers.insert(
				"KC-API-TIMESTAMP",
				header::HeaderValue::from_str(&timestamp.to_string()).map_err(|e| AuthError::Other(eyre!("Invalid timestamp: {}", e)))?,
			);
			headers.insert(
				"KC-API-PASSPHRASE",
				header::HeaderValue::from_str(&encrypted_passphrase).map_err(|e| AuthError::Other(eyre!("Invalid passphrase: {}", e)))?,
			);
			headers.insert("KC-API-KEY-VERSION", header::HeaderValue::from_static("2"));
			headers.insert("Content-Type", header::HeaderValue::from_static("application/json"));

			return Ok(request);
		}

		Ok(builder.build().expect("don't expect this to be reached by client, so fail fast for dev"))
	}

	fn handle_response(&self, status: StatusCode, _headers: HeaderMap, response_body: Bytes) -> Result<Self::Successful, HandleError> {
		if status.is_success() {
			// Kucoin returns HTTP 200 even for API errors, so we need to check code field
			let value: serde_json::Value = serde_json::from_slice(&response_body).map_err(|error| {
				let response_str = truncate_msg(String::from_utf8_lossy(&response_body));
				HandleError::Parse(eyre!("Failed to parse response: {error}\nResponse body: {response_str}"))
			})?;

			// Check if response contains code field
			if let Some(code) = value.get("code").and_then(|v| v.as_str()) {
				if code != "200000" {
					// Non-200000 code indicates an error
					let msg = value.get("msg").and_then(|v| v.as_str()).unwrap_or("Unknown error");
					let error = KucoinError {
						code: code.to_string(),
						msg: msg.to_string(),
					};
					return Err(ApiError::from(error).into());
				}
			}

			// No error, deserialize to the expected type
			serde_json::from_value(value.clone()).map_err(|error| {
				let response_str = truncate_msg(value.to_string());
				HandleError::Parse(eyre!("Failed to parse successful response: {error}\nResponse body: {response_str}"))
			})
		} else {
			let api_error: KucoinError = match serde_json::from_slice(&response_body) {
				Ok(parsed) => parsed,
				Err(error) => {
					let response_str = truncate_msg(String::from_utf8_lossy(&response_body));
					return Err(HandleError::Parse(eyre!("Failed to parse error response: {error}\nResponse body: {response_str}")));
				}
			};
			Err(ApiError::from(api_error).into())
		}
	}
}

// Ws stuff {{{
#[derive(Clone, Debug)]
pub struct KucoinWsHandler {
	options: KucoinOptions,
}
impl KucoinWsHandler {
	pub fn new(options: KucoinOptions) -> Self {
		Self { options }
	}
}
impl WsHandler for KucoinWsHandler {
	fn config(&self) -> Result<WsConfig, UrlError> {
		let mut config = self.options.ws_config.clone();
		if self.options.ws_url != KucoinWsUrl::None {
			config.base_url = match self.options.test {
				true => Some(self.options.ws_url.url_testnet().ok_or_else(|| UrlError::MissingTestnet(self.options.ws_url.url_mainnet()))?),
				false => Some(self.options.ws_url.url_mainnet()),
			}
		}
		config.topics = config.topics.union(&self.options.ws_topics).cloned().collect();
		Ok(config)
	}

	fn handle_auth(&mut self) -> Result<Vec<tungstenite::Message>, WsError> {
		if self.options.ws_config.auth {
			let _pubkey = self.options.pubkey.as_ref().ok_or(AuthError::MissingPubkey)?;
			let _secret = self.options.secret.as_ref().ok_or(AuthError::MissingSecret)?;
			//TODO: implement ws auth for kucoin
		}

		Ok(vec![])
	}

	fn handle_subscribe(&mut self, topics: HashSet<Topic>) -> Result<Vec<tungstenite::Message>, WsError> {
		let string_topics = topics
			.iter()
			.filter_map(|topic| if let Topic::String(s) = topic { Some(s) } else { None })
			.cloned()
			.collect::<Vec<_>>();
		let messages = {
			let msg = serde_json::json!({
				"type": "subscribe",
				"topic": string_topics.join(","),
				"response": true,
			});
			vec![tungstenite::Message::Text(msg.to_string().into())]
		};

		Ok(messages)
	}

	fn handle_jrpc(&mut self, jrpc: serde_json::Value) -> Result<ResponseOrContent, WsError> {
		// Basic structure for Kucoin websocket messages
		let event_type = jrpc.get("type").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
		let topic = jrpc.get("topic").and_then(|v| v.as_str()).unwrap_or("").to_string();
		let data = jrpc.get("data").cloned().unwrap_or(serde_json::Value::Null);
		let time_ms = jrpc.get("time").and_then(|v| v.as_i64()).unwrap_or(0);
		let time = Timestamp::from_millisecond(time_ms).unwrap_or_else(|_| Timestamp::now());

		let content = ContentEvent { data, topic, time, event_type };
		Ok(ResponseOrContent::Content(content))
	}
}
impl WsOption for KucoinOption {
	type WsHandler = KucoinWsHandler;

	fn ws_handler(options: Self::Options) -> Self::WsHandler {
		KucoinWsHandler::new(options)
	}
}
//,}}}

/// Options that can be set when creating handlers
#[derive(Debug, Default)]
pub enum KucoinOption {
	#[default]
	None,
	/// API key
	Pubkey(String),
	/// Api secret
	Secret(SecretString),
	/// API passphrase
	Passphrase(SecretString),
	/// Use testnet
	Test(bool),

	/// Base url for HTTP requests
	HttpUrl(KucoinHttpUrl),
	/// Authentication type for HTTP requests
	HttpAuth(KucoinAuth),

	/// Base url for WebSocket connections
	WsUrl(KucoinWsUrl),
	/// [WebSocketConfig] used for creating [WebSocketConnection]s
	WsConfig(WsConfig),
	/// See [WsConfig::topics]. Will be merged with those manually defined in [Self::WsConfig::topics], if any.
	WsTopics(Vec<String>),
}

/// A `enum` that represents the base url of the Kucoin REST API.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum KucoinHttpUrl {
	/// `https://api.kucoin.com`
	#[default]
	Spot,
	/// `https://api-futures.kucoin.com`
	Futures,
	/// The url will not be modified by [KucoinRequestHandler]
	None,
}
impl EndpointUrl for KucoinHttpUrl {
	fn url_mainnet(&self) -> Url {
		match self {
			Self::Spot => Url::parse("https://api.kucoin.com").unwrap(),
			Self::Futures => Url::parse("https://api-futures.kucoin.com").unwrap(),
			Self::None => Url::parse("").unwrap(),
		}
	}

	fn url_testnet(&self) -> Option<Url> {
		match self {
			Self::Spot => Some(Url::parse("https://openapi-sandbox.kucoin.com").unwrap()),
			Self::Futures => Some(Url::parse("https://api-sandbox-futures.kucoin.com").unwrap()),
			Self::None => Some(Url::parse("").unwrap()),
		}
	}
}

/// A `enum` that represents the base url of the Kucoin WebSocket API
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum KucoinWsUrl {
	/// `wss://ws-api-spot.kucoin.com`
	#[default]
	Spot,
	/// `wss://ws-api-futures.kucoin.com`
	Futures,
	/// The url will not be modified by [KucoinWsHandler]
	None,
}
impl EndpointUrl for KucoinWsUrl {
	fn url_mainnet(&self) -> url::Url {
		match self {
			Self::Spot => Url::parse("wss://ws-api-spot.kucoin.com").unwrap(),
			Self::Futures => Url::parse("wss://ws-api-futures.kucoin.com").unwrap(),
			Self::None => Url::parse("").unwrap(),
		}
	}

	fn url_testnet(&self) -> Option<url::Url> {
		match self {
			Self::Spot => Some(Url::parse("wss://ws-api-sandbox-spot.kucoin.com").unwrap()),
			Self::Futures => Some(Url::parse("wss://ws-api-sandbox-futures.kucoin.com").unwrap()),
			Self::None => Some(Url::parse("").unwrap()),
		}
	}
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum KucoinAuth {
	Sign,
	#[default]
	None,
}

/// A `struct` that implements [RequestHandler]
pub struct KucoinRequestHandler<'a, R: DeserializeOwned> {
	options: KucoinOptions,
	_phantom: PhantomData<&'a R>,
}

/// A `struct` that represents a set of [KucoinOption] s.
#[derive(Clone, derive_more::Debug, Default)]
pub struct KucoinOptions {
	/// see [KucoinOption::Pubkey]
	pub pubkey: Option<String>,
	/// see [KucoinOption::Secret]
	#[debug("[REDACTED]")]
	pub secret: Option<SecretString>,
	/// see [KucoinOption::Passphrase]
	#[debug("[REDACTED]")]
	pub passphrase: Option<SecretString>,
	/// see [KucoinOption::HttpUrl]
	pub http_url: KucoinHttpUrl,
	/// see [KucoinOption::HttpAuth]
	pub http_auth: KucoinAuth,
	/// see [KucoinOption::WsUrl]
	pub ws_url: KucoinWsUrl,
	/// see [KucoinOption::WsConfig]
	pub ws_config: WsConfig,
	/// see [KucoinOption::WsTopics]
	pub ws_topics: HashSet<String>,
	/// see [KucoinOption::Test]
	pub test: bool,
}
impl HandlerOptions for KucoinOptions {
	type OptionItem = KucoinOption;

	fn update(&mut self, option: Self::OptionItem) {
		match option {
			Self::OptionItem::None => (),
			Self::OptionItem::Pubkey(v) => self.pubkey = Some(v),
			Self::OptionItem::Secret(v) => self.secret = Some(v),
			Self::OptionItem::Passphrase(v) => self.passphrase = Some(v),
			Self::OptionItem::Test(v) => self.test = v,
			Self::OptionItem::HttpUrl(v) => self.http_url = v,
			Self::OptionItem::HttpAuth(v) => self.http_auth = v,
			Self::OptionItem::WsUrl(v) => self.ws_url = v,
			Self::OptionItem::WsConfig(v) => self.ws_config = v,
			Self::OptionItem::WsTopics(v) => self.ws_topics = v.into_iter().collect(),
		}
	}

	fn is_authenticated(&self) -> bool {
		self.pubkey.is_some() && self.secret.is_some() && self.passphrase.is_some()
	}
}

impl<'a, R, B> HttpOption<'a, R, B> for KucoinOption
where
	R: DeserializeOwned + 'a,
	B: Serialize,
{
	type RequestHandler = KucoinRequestHandler<'a, R>;

	fn request_handler(options: Self::Options) -> Self::RequestHandler {
		KucoinRequestHandler::<'a, R> { options, _phantom: PhantomData }
	}
}

impl HandlerOption for KucoinOption {
	type Options = KucoinOptions;
}

// Error Codes {{{
#[derive(Clone, Debug, Deserialize)]
pub struct KucoinError {
	pub code: String,
	pub msg: String,
}
impl From<KucoinError> for ApiError {
	fn from(e: KucoinError) -> Self {
		//HACK
		eyre!("Kucoin API error {}: {}", e.code, e.msg).into()
	}
}
//,}}}
