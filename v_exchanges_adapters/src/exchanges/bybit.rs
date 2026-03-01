//! A module for communicating with the [Bybit API](https://bybit-exchange.github.io/docs/spot/v3/#t-introduction).
//! For example usages, see files in the examples/ directory.

use std::{borrow::Cow, marker::PhantomData, time::SystemTime, vec};

use generics::{ConstructAuthError, UrlError, tokio_tungstenite::tungstenite};
use hmac::{Hmac, Mac};
use jiff::Timestamp;
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::json;
use sha2::Sha256;
use url::Url;
use v_exchanges_api_generics::{
	http::{header::HeaderValue, *},
	ws::*,
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
	/// Use testnet
	Testnet(bool),

	/// Base url for HTTP requests
	HttpUrl(BybitHttpUrl),
	/// Type of authentication used for HTTP requests.
	HttpAuth(BybitHttpAuth),
	/// receive window parameter used for requests
	RecvWindow(std::time::Duration),
	/// Base url for Ws connections
	WsUrl(BybitWsUrlBase),
	/// Whether [BybitWsHandler] should perform authentication
	WsAuth(bool),
	/// [WsConfig] used for creating [WsConnection]s
	/// `url_prefix` will be overridden by [WsUrl](Self::WsUrl) unless `WsUrl` is [BybitWsUrl::None].
	/// By default, `ignore_duplicate_during_reconnection` is set to `true`.
	WsConfig(WsConfig),
	/// Ref [WsConfig::topics]
	WsTopics(Vec<String>),
}

/// A `struct` that represents a set of [BybitOption] s.
#[derive(Clone, derive_more::Debug, Default)]
pub struct BybitOptions {
	/// see [BybitOption::Key]
	pub pubkey: Option<String>,
	/// see [BybitOption::Secret]
	#[debug("[REDACTED]")]
	pub secret: Option<SecretString>,
	/// see [BybitOption::Testnet]
	pub testnet: bool,
	/// see [BybitOption::HttpUrl]
	pub http_url: BybitHttpUrl,
	/// see [BybitOption::HttpAuth]
	pub http_auth: BybitHttpAuth,
	/// see [BybitOption::RecvWindow]
	pub recv_window: Option<std::time::Duration>,
	/// see [BybitOption::WsUrl]
	pub ws_url: BybitWsUrlBase,
	/// see [BybitOption::WsAuth]
	pub ws_auth: bool,
	/// see [BybitOption::WsConfig]
	pub ws_config: WsConfig,
	/// see [BybitOption::WsTopics]
	pub ws_topics: HashSet<String>,
}

/// A `enum` that represents the base url of the Bybit REST API.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BybitHttpUrl {
	/// `https://api.bybit.com`
	#[default]
	Bybit,
	/// `https://api.bytick.com`
	Bytick,
	/// The url will not be modified by [BybitRequestHandler]
	None,
}
impl EndpointUrl for BybitHttpUrl {
	fn url_mainnet(&self) -> Url {
		match self {
			Self::Bybit => Url::parse("https://api.bybit.com").unwrap(),
			Self::Bytick => Url::parse("https://api.bytick.com").unwrap(),
			Self::None => Url::parse("").unwrap(),
		}
	}

	fn url_testnet(&self) -> Option<Url> {
		match self {
			Self::Bybit => Some(Url::parse("https://api-testnet.bybit.com").unwrap()),
			Self::Bytick => None, //HACK: maybe it has it, idk, needs checking
			Self::None => Some(Url::parse("").unwrap()),
		}
	}
}

/// Represents the auth type.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
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
	code: BybitErrorCode,
	msg: String,
}
impl From<BybitError> for ApiError {
	fn from(e: BybitError) -> Self {
		use v_exchanges_api_generics::http::AuthError;
		match e.code {
			BybitErrorCode::ApiKeyExpired(_) => AuthError::KeyExpired { msg: e.msg }.into(),
			BybitErrorCode::InvalidApiKey(_) | BybitErrorCode::ErrorSign(_) | BybitErrorCode::AuthenticationFailed(_) | BybitErrorCode::UnmatchedIp(_) =>
				AuthError::Unauthorized { msg: e.msg }.into(),
			BybitErrorCode::PermissionDenied(_) => AuthError::Unauthorized { msg: e.msg }.into(),
			_ => ApiError::Other(eyre!("Bybit error {}: {}", e.code.as_i32(), e.msg)),
		}
	}
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(from = "i32", into = "i32")]
pub enum BybitErrorCode {
	// 10xxx - General / Auth
	InvalidApiKey(i32),
	ErrorSign(i32),
	PermissionDenied(i32),
	TooManyVisits(i32),
	AuthenticationFailed(i32),
	IpBanned(i32),
	UnmatchedIp(i32),
	InvalidDuplicateRequest(i32),
	ServerError(i32),
	RouteNotFound(i32),
	IpRateLimit(i32),
	ComplianceRules(i32),

	// 33xxx - Derivatives-specific
	ApiKeyExpired(i32),

	// 110xxx - Order/Position errors
	OrderNotExist(i32),
	InsufficientBalance(i32),

	#[default]
	Ok,
	Other(i32),
}

impl BybitErrorCode {
	fn as_i32(self) -> i32 {
		match self {
			Self::Ok => 0,
			Self::InvalidApiKey(c)
			| Self::ErrorSign(c)
			| Self::PermissionDenied(c)
			| Self::TooManyVisits(c)
			| Self::AuthenticationFailed(c)
			| Self::IpBanned(c)
			| Self::UnmatchedIp(c)
			| Self::InvalidDuplicateRequest(c)
			| Self::ServerError(c)
			| Self::RouteNotFound(c)
			| Self::IpRateLimit(c)
			| Self::ComplianceRules(c)
			| Self::ApiKeyExpired(c)
			| Self::OrderNotExist(c)
			| Self::InsufficientBalance(c)
			| Self::Other(c) => c,
		}
	}
}

impl From<i32> for BybitErrorCode {
	fn from(code: i32) -> Self {
		match code {
			0 => Self::Ok,
			10003 => Self::InvalidApiKey(code),
			10004 => Self::ErrorSign(code),
			10005 => Self::PermissionDenied(code),
			10006 => Self::TooManyVisits(code),
			10007 => Self::AuthenticationFailed(code),
			10009 => Self::IpBanned(code),
			10010 => Self::UnmatchedIp(code),
			10014 => Self::InvalidDuplicateRequest(code),
			10016 => Self::ServerError(code),
			10017 => Self::RouteNotFound(code),
			10018 => Self::IpRateLimit(code),
			10024 => Self::ComplianceRules(code),
			33004 => Self::ApiKeyExpired(code),
			110001 => Self::OrderNotExist(code),
			110007 => Self::InsufficientBalance(code),
			code => {
				tracing::warn!("Encountered unknown Bybit error code: {code}");
				Self::Other(code)
			}
		}
	}
}

impl From<BybitErrorCode> for i32 {
	fn from(code: BybitErrorCode) -> Self {
		code.as_i32()
	}
}

/// A `struct` that implements [RequestHandler]
pub struct BybitRequestHandler<'a, R: DeserializeOwned> {
	options: BybitOptions,
	_phantom: PhantomData<&'a R>,
}

impl<B, R> RequestHandler<B> for BybitRequestHandler<'_, R>
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

	fn build_request(&self, mut builder: RequestBuilder, request_body: &Option<B>, _: u8) -> Result<Request, BuildError> {
		if self.options.http_auth == BybitHttpAuth::None {
			if let Some(body) = request_body {
				let json = serde_json::to_string(body)?;
				builder = builder.header(header::CONTENT_TYPE, "application/json").body(json);
			}
			return Ok(builder.build().expect("My understanding is client can't trigger this. So fail fast for dev"));
		}

		let pubkey = self.options.pubkey.as_deref().ok_or(ConstructAuthError::MissingPubkey)?;
		let secret = self.options.secret.as_ref().map(|s| s.expose_secret()).ok_or(ConstructAuthError::MissingSecret)?;

		let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap(); // always after the epoch
		let timestamp = time.as_millis();

		let hmac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap(); // hmac accepts key of any length

		match self.options.http_auth {
			BybitHttpAuth::SpotV1 => Self::v1_auth(builder, request_body, pubkey, timestamp, hmac, true, self.options.recv_window),
			BybitHttpAuth::BelowV3 => Self::v1_auth(builder, request_body, pubkey, timestamp, hmac, false, self.options.recv_window),
			BybitHttpAuth::UsdcContractV1 => Self::v3_auth(builder, request_body, pubkey, timestamp, hmac, true, self.options.recv_window),
			BybitHttpAuth::V3AndAbove => Self::v3_auth(builder, request_body, pubkey, timestamp, hmac, false, self.options.recv_window),
			BybitHttpAuth::None => unreachable!("we're already handled this case"),
		}
	}

	fn handle_response(&self, status: StatusCode, _: HeaderMap, response_body: Bytes) -> Result<Self::Successful, HandleError> {
		if status.is_success() {
			// Bybit returns HTTP 200 even for API errors, so we need to check retCode
			// First, try to parse as a generic response to check for errors
			let value: serde_json::Value = serde_json::from_slice(&response_body).map_err(|error| {
				let response_str = v_utils::utils::truncate_msg(String::from_utf8_lossy(&response_body));
				HandleError::Parse(eyre!("Failed to parse response: {error}\nResponse body: {response_str}"))
			})?;

			// Check if response contains retCode field (V3/V5 API format)
			if let Some(ret_code) = value.get("retCode").and_then(|v| v.as_i64())
				&& ret_code != 0
			{
				// Non-zero retCode indicates an error
				let ret_msg = value.get("retMsg").and_then(|v| v.as_str()).unwrap_or("Unknown error");
				let error = BybitError {
					code: BybitErrorCode::from(ret_code as i32),
					msg: ret_msg.to_string(),
				};
				return Err(ApiError::from(error).into());
			}

			// No error, deserialize to the expected type
			serde_json::from_value(value.clone()).map_err(|error| {
				let response_str = v_utils::utils::truncate_msg(value.to_string());
				HandleError::Parse(eyre!("Failed to parse successful response: {error}\nResponse body: {response_str}"))
			})
		} else {
			if status == 403 {
				return Err(ApiError::IpTimeout { until: None }.into());
			}
			if status == 401 {
				use v_exchanges_api_generics::http::AuthError;
				let msg = match std::str::from_utf8(&response_body) {
					Ok(s) if !s.is_empty() => s.to_string(),
					_ => "HTTP 401 Unauthorized".to_string(),
				};
				return Err(ApiError::Auth(AuthError::Unauthorized { msg }).into());
			}
			// https://bybit-exchange.github.io/docs/spot/v3/#t-ratelimits
			let api_error: BybitError = match serde_json::from_slice(&response_body) {
				Ok(parsed) => parsed,
				Err(error) => {
					let response_str = v_utils::utils::truncate_msg(String::from_utf8_lossy(&response_body));
					return Err(HandleError::Parse(eyre!("Failed to parse error response: {error}\nResponse body: {response_str}")));
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
	fn v1_auth<B>(
		builder: RequestBuilder,
		request_body: &Option<B>,
		key: &str,
		timestamp: u128,
		mut hmac: Hmac<Sha256>,
		spot: bool,
		window: Option<std::time::Duration>,
	) -> Result<Request, BuildError>
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
				let window_ms = window.as_millis() as u64;
				if spot {
					queries.push((Cow::Borrowed("recvWindow"), Cow::Owned(window_ms.to_string())));
				} else {
					queries.push((Cow::Borrowed("recv_window"), Cow::Owned(window_ms.to_string())));
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
				let window_ms = window.as_millis() as u64;
				if !body.is_empty() {
					body.push('&');
				}
				if spot {
					body.push_str("recvWindow=");
				} else {
					body.push_str("recv_window=");
				}
				body.push_str(&window_ms.to_string());
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
		window: Option<std::time::Duration>,
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
			let window_ms = window.as_millis() as u64;
			sign_contents.push_str(&window_ms.to_string());
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
		headers.insert(
			"X-BAPI-API-KEY",
			HeaderValue::from_str(key).or(Err(ConstructAuthError::InvalidCharacterInApiKey(key.to_owned())))?,
		);
		headers.insert("X-BAPI-TIMESTAMP", HeaderValue::from(timestamp as u64));
		if let Some(window) = window {
			let window_ms = window.as_millis() as u64;
			headers.insert("X-BAPI-RECV-WINDOW", HeaderValue::from(window_ms));
		}
		Ok(request)
	}
}

// Ws stuff {{{
#[derive(Debug, derive_new::new)]
pub struct BybitWsHandler {
	options: BybitOptions,
}
impl WsHandler for BybitWsHandler {
	fn config(&self) -> Result<WsConfig, UrlError> {
		let mut config = self.options.ws_config.clone();
		if self.options.ws_url != BybitWsUrlBase::None {
			config.base_url = match self.options.testnet {
				true => Some(self.options.ws_url.url_testnet().ok_or_else(|| UrlError::MissingTestnet(self.options.ws_url.url_mainnet()))?),
				false => Some(self.options.ws_url.url_mainnet()),
			}
		}
		config.topics = config.topics.union(&self.options.ws_topics).cloned().collect();
		Ok(config)
	}

	#[instrument(skip_all)]
	fn handle_auth(&mut self) -> Result<Vec<tungstenite::Message>, WsError> {
		match self.options.ws_auth {
			true => {
				let pubkey = self.options.pubkey.as_ref().ok_or(ConstructAuthError::MissingPubkey)?;
				let secret = self.options.secret.as_ref().ok_or(ConstructAuthError::MissingSecret)?;
				let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).expect("always after the epoch");
				//XXX: expiration time here is hardcoded to 1s, which would override any specifications of a longer recv_window on top.
				let expires = time.as_millis() as u64 + 1000; //TODO: figure out how large can I make this

				// sign with HMAC-SHA256
				let mut hmac = Hmac::<Sha256>::new_from_slice(secret.expose_secret().as_bytes()).expect("hmac accepts key of any length");
				hmac.update(format!("GET/realtime{expires}").as_bytes());
				let signature = hex::encode(hmac.finalize().into_bytes());

				Ok(vec![tungstenite::Message::Text(
					json!({
						"op": "auth",
						"args": [pubkey, expires, signature],
					})
					.to_string()
					.into(),
				)])
			}
			false => self.handle_subscribe(self.options.ws_topics.iter().cloned().map(Topic::String).collect()),
		}
	}

	fn handle_subscribe(&mut self, topics: HashSet<Topic>) -> Result<Vec<tungstenite::Message>, WsError> {
		let topics: Vec<String> = topics
			.into_iter()
			.map(|topic| match topic {
				Topic::String(s) => s,
				Topic::Order(_) => todo!(),
			})
			.collect();
		Ok(vec![tungstenite::Message::Text(json!({ "op": "subscribe", "args": topics }).to_string().into())])
	}

	#[instrument(skip_all, fields(jrpc = ?format_args!("{:#?}", jrpc)))]
	fn handle_jrpc(&mut self, jrpc: serde_json::Value) -> Result<ResponseOrContent, WsError> {
		//TODO!!!!!!!!!!!: tell serde that enum name is not part of it
		#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
		#[serde(untagged)]
		enum BybitResponse {
			Feedback(FeedbackResponse),
			Content(ContentResponse),
		}
		//HACK: this ignores differences of `Option` endpoints: https://bybit-exchange.github.io/docs/v5/ws/connect#:~:text=Linear/Inverse-,Option,-%7B%0A%20%20%20%20%22success%22%3A%20true%2C%0A%20%20%20%20%22conn_id
		#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
		struct FeedbackResponse {
			success: bool,
			ret_msg: String,
			op: Operation,
			/// returned if was specified in the request
			req_id: Option<String>,
			conn_id: String,
		}
		#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
		#[serde(rename_all = "lowercase")]
		enum Operation {
			Auth,
			Subscribe,
			Unsubscribe,
		}
		#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
		pub struct ContentResponse {
			pub data: serde_json::Value,
			pub topic: String,
			pub ts: i64,
			#[serde(rename = "type")]
			pub event_type: String,
		}
		impl From<ContentResponse> for ContentEvent {
			fn from(content: ContentResponse) -> Self {
				ContentEvent {
					topic: content.topic,
					data: content.data,
					time: Timestamp::from_millisecond(content.ts).unwrap(),
					event_type: content.event_type,
				}
			}
		}

		let bybit_response = serde_json::from_value::<BybitResponse>(jrpc.clone()).wrap_err_with(|| format!("Failed to deserialize Bybit response: {jrpc:?}"))?;
		match bybit_response {
			BybitResponse::Feedback(FeedbackResponse { op, success, ret_msg, .. }) => match op {
				Operation::Auth => match success {
					true => {
						tracing::info!("Ws authentication successful");
						Ok(ResponseOrContent::Response(
							self.handle_subscribe(self.options.ws_topics.clone().into_iter().map(Topic::String).collect())?,
						))
					}
					false => Err(ConstructAuthError::Other(eyre!("Authentication was not successful: {ret_msg}")).into()),
				},
				Operation::Subscribe => {
					if jrpc["success"].as_bool() == Some(true) {
						tracing::info!("Ws topics subscription successful");
					} else {
						match self.options.ws_auth || &ret_msg != "Request not authorized" {
							true => return Err(WsError::Subscription(ret_msg)),
							false => {
								return Err(ConstructAuthError::Other(eyre!("Tried to access a private endpoint without authentication")).into());
							}
						}
					}
					Ok(ResponseOrContent::Response(vec![]))
				}
				_ => todo!(),
			},
			BybitResponse::Content(content) => Ok(ResponseOrContent::Content(ContentEvent::from(content))),
		}
	}
}
/// A `enum` that represents the base url of the Bybit Ws API.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BybitWsUrlBase {
	/// `wss://stream.bybit.com`
	#[default]
	Bybit,
	/// `wss://stream.bytick.com`
	Bytick,
	/// The url will not be modified by [BybitWsHandler]
	None,
}
impl EndpointUrl for BybitWsUrlBase {
	fn url_mainnet(&self) -> Url {
		match self {
			Self::Bybit => Url::parse("wss://stream.bybit.com").unwrap(),
			Self::Bytick => Url::parse("wss://stream.bytick.com").unwrap(),
			Self::None => Url::parse("").unwrap(),
		}
	}

	fn url_testnet(&self) -> Option<Url> {
		match self {
			Self::Bybit => Some(Url::parse("wss://stream-testnet.bybit.com").unwrap()),
			Self::Bytick => None, //HACK: no clue if it actually exists, but don't care rn
			Self::None => Some(Url::parse("").unwrap()),
		}
	}
}
impl WsOption for BybitOption {
	type WsHandler = BybitWsHandler;

	fn ws_handler(options: Self::Options) -> Self::WsHandler {
		BybitWsHandler::new(options)
	}
}
//,}}}

impl HandlerOptions for BybitOptions {
	type OptionItem = BybitOption;

	fn update(&mut self, option: Self::OptionItem) {
		match option {
			BybitOption::None => (),
			BybitOption::Pubkey(v) => self.pubkey = Some(v),
			BybitOption::Secret(v) => self.secret = Some(v),
			BybitOption::Testnet(v) => self.testnet = v,
			BybitOption::HttpUrl(v) => self.http_url = v,
			BybitOption::HttpAuth(v) => self.http_auth = v,
			BybitOption::RecvWindow(v) => self.recv_window = Some(v),
			BybitOption::WsUrl(v) => self.ws_url = v,
			BybitOption::WsAuth(v) => self.ws_auth = v,
			BybitOption::WsConfig(v) => self.ws_config = v,
			BybitOption::WsTopics(v) => self.ws_topics = v.into_iter().collect(),
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

	fn request_handler(options: Self::Options) -> Self::RequestHandler {
		BybitRequestHandler::<'a, R> { options, _phantom: PhantomData }
	}
}

impl HandlerOption for BybitOption {
	type Options = BybitOptions;
}
