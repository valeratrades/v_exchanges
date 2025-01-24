// A module for communicating with the [Binance API](https://binance-docs.github.io/apidocs/spot/en/).

use std::{
	marker::PhantomData,
	str::FromStr,
	time::{Duration, SystemTime},
};

use hmac::{Hmac, Mac};
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::Sha256;
use v_exchanges_api_generics::{http::*, websocket::*};

use crate::traits::*;

/// The type returned by [Client::request()].
pub type BinanceRequestResult<T> = Result<T, BinanceRequestError>;
pub type BinanceRequestError = RequestError<&'static str, BinanceHandlerError>;

/// Options that can be set when creating handlers
pub enum BinanceOption {
	/// [Default] variant, does nothing
	Default,
	/// API key
	Key(String),
	/// Api secret
	Secret(SecretString),
	/// Number of milliseconds the request is valid for. Only applicable for signed requests.
	RecvWindow(u16),
	/// Base url for HTTP requests
	HttpUrl(BinanceHttpUrl),
	/// Authentication type for HTTP requests
	HttpAuth(BinanceAuth),
	/// [RequestConfig] used when sending requests.
	/// `url_prefix` will be overridden by [HttpUrl](Self::HttpUrl) unless `HttpUrl` is [BinanceHttpUrl::None].
	RequestConfig(RequestConfig),
	/// Base url for WebSocket connections
	WebSocketUrl(BinanceWebSocketUrl),
	/// [WebSocketConfig] used for creating [WebSocketConnection]s
	/// `url_prefix` will be overridden by [WebSocketUrl](Self::WebSocketUrl) unless `WebSocketUrl` is [BinanceWebSocketUrl::None].
	/// By default, `refresh_after` is set to 12 hours and `ignore_duplicate_during_reconnection` is set to `true`.
	WebSocketConfig(WebSocketConfig),
}

/// A `struct` that represents a set of [BinanceOption] s.
#[derive(Clone, derive_more::Debug)]
pub struct BinanceOptions {
	/// see [BinanceOption::Key]
	pub key: Option<String>,
	/// see [BinanceOption::Secret]
	#[debug("[REDACTED]")]
	pub secret: Option<SecretString>,
	// see [BinanceOption::RecvWindow]
	pub recv_window: Option<u16>,
	/// see [BinanceOption::HttpUrl]
	pub http_url: BinanceHttpUrl,
	/// see [BinanceOption::HttpAuth]
	pub http_auth: BinanceAuth,
	/// see [BinanceOption::RequestConfig]
	pub request_config: RequestConfig,
	/// see [BinanceOption::WebSocketUrl]
	pub websocket_url: BinanceWebSocketUrl,
	/// see [BinanceOption::WebSocketConfig]
	pub websocket_config: WebSocketConfig,
}

/// A `enum` that represents the base url of the Binance REST API.
#[derive(Debug, Eq, PartialEq, Copy, Clone, Default)]
#[non_exhaustive]
pub enum BinanceHttpUrl {
	/// `https://api.binance.com`
	Spot,
	/// `https://api1.binance.com`
	Spot1,
	/// `https://api2.binance.com`
	Spot2,
	/// `https://api3.binance.com`
	Spot3,
	/// `https://api4.binance.com`
	Spot4,
	/// `https://testnet.binance.vision`
	SpotTest,
	/// `https://data.binance.com`
	SpotData,
	/// `https://fapi.binance.com`
	FuturesUsdM,
	/// `https://dapi.binance.com`
	FuturesCoinM,
	/// `https://testnet.binancefuture.com`
	FuturesTest,
	/// `https://eapi.binance.com`
	EuropeanOptions,
	/// The url will not be modified by [BinanceRequestHandler]
	#[default]
	None,
}

/// A `enum` that represents the base url of the Binance WebSocket API
#[derive(Debug, Eq, PartialEq, Copy, Clone, Default)]
#[non_exhaustive]
pub enum BinanceWebSocketUrl {
	/// `wss://stream.binance.com:9443`
	Spot9443,
	/// `wss://stream.binance.com:443`
	Spot443,
	/// `wss://testnet.binance.vision`
	SpotTest,
	/// `wss://data-stream.binance.com`
	SpotData,
	/// `wss://ws-api.binance.com:443`
	WebSocket443,
	/// `wss://ws-api.binance.com:9443`
	WebSocket9443,
	/// `wss://fstream.binance.com`
	FuturesUsdM,
	/// `wss://fstream-auth.binance.com`
	FuturesUsdMAuth,
	/// `wss://dstream.binance.com`
	FuturesCoinM,
	/// `wss://stream.binancefuture.com`
	FuturesUsdMTest,
	/// `wss://dstream.binancefuture.com`
	FuturesCoinMTest,
	/// `wss://nbstream.binance.com`
	EuropeanOptions,
	/// The url will not be modified by [BinanceRequestHandler]
	#[default]
	None,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Default)]
pub enum BinanceAuth {
	Sign,
	Key,
	#[default]
	None,
}

#[derive(Debug)]
pub enum BinanceHandlerError {
	ApiError(BinanceError),
	RateLimitError { retry_after: Option<u32> },
	ParseError,
}

#[derive(Deserialize, Debug)]
pub struct BinanceError {
	pub code: i32,
	pub msg: String,
}

/// A `struct` that implements [RequestHandler]
pub struct BinanceRequestHandler<'a, R: DeserializeOwned> {
	options: BinanceOptions,
	_phantom: PhantomData<&'a R>,
}

/// A `struct` that implements [WebSocketHandler]
pub struct BinanceWebSocketHandler {
	message_handler: Box<dyn FnMut(serde_json::Value) + Send>,
	options: BinanceOptions,
}

// https://binance-docs.github.io/apidocs/spot/en/#general-api-information
impl<B, R> RequestHandler<B> for BinanceRequestHandler<'_, R>
where
	B: Serialize,
	R: DeserializeOwned,
{
	type BuildError = &'static str;
	type Successful = R;
	type Unsuccessful = BinanceHandlerError;

	fn base_url(&self) -> String {
		self.options.http_url.as_str().to_owned()
	}

	#[tracing::instrument(skip_all, fields(?builder))]
	fn build_request(&self, mut builder: RequestBuilder, request_body: &Option<B>, _: u8) -> Result<Request, Self::BuildError> {
		if let Some(body) = request_body {
			let encoded = serde_urlencoded::to_string(body).or(Err("could not serialize body as application/x-www-form-urlencoded"))?;
			builder = builder.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(encoded);
		}

		if self.options.http_auth != BinanceAuth::None {
			// https://binance-docs.github.io/apidocs/spot/en/#signed-trade-user_data-and-margin-endpoint-security
			let key = self.options.key.as_deref().ok_or("API key not set")?;
			builder = builder.header("X-MBX-APIKEY", key);

			if self.options.http_auth == BinanceAuth::Sign {
				let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap(); // always after the epoch
				let timestamp = time.as_millis();

				builder = builder.query(&[("timestamp", timestamp)]);
				if let Some(recv_window) = self.options.recv_window {
					builder = builder.query(&[("recvWindow", recv_window)]);
				}

				let secret = self.options.secret.as_ref().map(|s| s.expose_secret()).ok_or("API secret not set")?;
				let mut hmac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap(); // hmac accepts key of any length

				let mut request = builder.build().or(Err("Failed to build request"))?;
				let query = request.url().query().unwrap(); // we added the timestamp query
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
				BinanceHandlerError::ParseError
			})
		} else {
			// https://binance-docs.github.io/apidocs/spot/en/#limits
			//TODO: error parsing from status
			//let error_code = BinanceErrorCode::from(status.as_u16());
			//XXX: binance doesn't even return these
			if status == 429 || status == 418 {
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
				return Err(BinanceHandlerError::RateLimitError { retry_after });
			}

			let error = match serde_json::from_slice(&response_body) {
				Ok(parsed_error) => BinanceHandlerError::ApiError(parsed_error),
				Err(error) => {
					tracing::debug!("Failed to parse error response due to an error: {}", error);
					BinanceHandlerError::ParseError
				}
			};
			Err(error)
		}
	}
}

impl WebSocketHandler for BinanceWebSocketHandler {
	fn websocket_config(&self) -> WebSocketConfig {
		let mut config = self.options.websocket_config.clone();
		if self.options.websocket_url != BinanceWebSocketUrl::None {
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

impl BinanceHttpUrl {
	/// The URL that this variant represents.
	#[inline(always)]
	fn as_str(&self) -> &'static str {
		match self {
			Self::Spot => "https://api.binance.com",
			Self::Spot1 => "https://api1.binance.com",
			Self::Spot2 => "https://api2.binance.com",
			Self::Spot3 => "https://api3.binance.com",
			Self::Spot4 => "https://api4.binance.com",
			Self::SpotTest => "https://testnet.binance.vision",
			Self::SpotData => "https://data.binance.com",
			Self::FuturesUsdM => "https://fapi.binance.com",
			Self::FuturesCoinM => "https://dapi.binance.com",
			Self::FuturesTest => "https://testnet.binancefuture.com",
			Self::EuropeanOptions => "https://eapi.binance.com",
			Self::None => "",
		}
	}
}

impl BinanceWebSocketUrl {
	/// The URL that this variant represents.
	#[inline(always)]
	pub fn as_str(&self) -> &'static str {
		match self {
			Self::Spot9443 => "wss://stream.binance.com:9443",
			Self::Spot443 => "wss://stream.binance.com:443",
			Self::SpotTest => "wss://testnet.binance.vision",
			Self::SpotData => "wss://data-stream.binance.com",
			Self::WebSocket443 => "wss://ws-api.binance.com:443",
			Self::WebSocket9443 => "wss://ws-api.binance.com:9443",
			Self::FuturesUsdM => "wss://fstream.binance.com",
			Self::FuturesUsdMAuth => "wss://fstream-auth.binance.com",
			Self::FuturesCoinM => "wss://dstream.binance.com",
			Self::FuturesUsdMTest => "wss://stream.binancefuture.com",
			Self::FuturesCoinMTest => "wss://dstream.binancefuture.com",
			Self::EuropeanOptions => "wss://nbstream.binance.com",
			Self::None => "",
		}
	}
}

impl HandlerOptions for BinanceOptions {
	type OptionItem = BinanceOption;

	fn update(&mut self, option: Self::OptionItem) {
		match option {
			BinanceOption::Default => (),
			BinanceOption::Key(v) => self.key = Some(v),
			BinanceOption::RecvWindow(v) => self.recv_window = Some(v),
			BinanceOption::Secret(v) => self.secret = Some(v),
			BinanceOption::HttpUrl(v) => self.http_url = v,
			BinanceOption::HttpAuth(v) => self.http_auth = v,
			BinanceOption::RequestConfig(v) => self.request_config = v,
			BinanceOption::WebSocketUrl(v) => self.websocket_url = v,
			BinanceOption::WebSocketConfig(v) => self.websocket_config = v,
		}
	}

	fn is_authenticated(&self) -> bool {
		self.key.is_some() // some end points are satisfied with just the key, and it's really difficult to provide only a key without a secret from the clientside, so assume intent if it's missing.
	}
}

impl Default for BinanceOptions {
	fn default() -> Self {
		let mut websocket_config = WebSocketConfig::new();
		websocket_config.refresh_after = Duration::from_secs(60 * 60 * 12);
		websocket_config.ignore_duplicate_during_reconnection = true;
		Self {
			key: None,
			secret: None,
			recv_window: None,
			http_url: BinanceHttpUrl::default(),
			http_auth: BinanceAuth::default(),
			request_config: RequestConfig::default(),
			websocket_url: BinanceWebSocketUrl::default(),
			websocket_config,
		}
	}
}

impl<'a, R, B> HttpOption<'a, R, B> for BinanceOption
where
	R: DeserializeOwned + 'a,
	B: Serialize,
{
	type RequestHandler = BinanceRequestHandler<'a, R>;

	#[inline(always)]
	fn request_handler(options: Self::Options) -> Self::RequestHandler {
		BinanceRequestHandler::<'a, R> { options, _phantom: PhantomData }
	}
}

impl<H: FnMut(serde_json::Value) + Send + 'static> WebSocketOption<H> for BinanceOption {
	type WebSocketHandler = BinanceWebSocketHandler;

	#[inline(always)]
	fn websocket_handler(handler: H, options: Self::Options) -> Self::WebSocketHandler {
		BinanceWebSocketHandler {
			message_handler: Box::new(handler),
			options,
		}
	}
}

impl HandlerOption for BinanceOption {
	type Options = BinanceOptions;
}

impl Default for BinanceOption {
	fn default() -> Self {
		Self::Default
	}
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(from = "i32")]
pub enum BinanceErrorCode {
	// 10xx - General Server/Network
	Unknown,
	Disconnected,
	Unauthorized,
	TooManyRequests,
	UnexpectedResponse,
	Timeout,
	ServerBusy,
	InvalidMessage,
	UnknownOrderComposition,
	TooManyOrders,
	ServiceShuttingDown,
	UnsupportedOperation,
	InvalidTimestamp,
	InvalidSignature,

	// 11xx - Request issues
	IllegalChars,
	TooManyParameters,
	MandatoryParamEmptyOrMalformed,
	UnknownParam,
	UnreadParameters,
	ParamEmpty,
	ParamNotRequired,
	ParamOverflow,
	BadPrecision,
	NoDepth,
	TifNotRequired,
	InvalidTif,
	InvalidOrderType,
	InvalidSide,
	EmptyNewClOrdId,
	EmptyOrgClOrdId,
	BadInterval,
	BadSymbol,
	InvalidSymbolStatus,
	InvalidListenKey,
	MoreThanXXHours,
	OptionalParamsBadCombo,
	InvalidParameter,
	BadStrategyType,
	InvalidJson,
	InvalidTickerType,
	InvalidCancelRestrictions,
	DuplicateSymbols,
	InvalidSbeHeader,
	UnsupportedSchemaId,
	SbeDisabled,
	OcoOrderTypeRejected,
	OcoIcebergqtyTimeinforce,
	DeprecatedSchema,
	BuyOcoLimitMustBeBelow,
	SellOcoLimitMustBeAbove,
	BothOcoOrdersCannotBeLimit,
	InvalidTagNumber,
	TagNotDefinedInMessage,
	TagAppearsMoreThanOnce,
	TagOutOfOrder,
	GroupFieldsOutOfOrder,
	InvalidComponent,
	ResetSeqNumSupport,
	AlreadyLoggedIn,
	GarbledMessage,
	BadSenderCompid,
	BadSeqNum,
	ExpectedLogon,
	TooManyMessages,
	ParamsBadCombo,
	NotAllowedInDropCopySessions,
	DropCopySessionNotAllowed,
	DropCopySessionRequired,
	NotAllowedInOrderEntrySessions,
	NotAllowedInMarketDataSessions,
	IncorrectNumInGroupCount,
	DuplicateEntriesInAGroup,
	InvalidRequestId,
	TooManySubscriptions,
	BuyOcoStopLossMustBeAbove,
	SellOcoStopLossMustBeBelow,
	BuyOcoTakeProfitMustBeBelow,
	SellOcoTakeProfitMustBeAbove,

	// 20xx - Business logic errors
	NewOrderRejected,
	CancelRejected,
	NoSuchOrder,
	BadApiKeyFmt,
	RejectedMbxKey,
	NoTradingWindow,
	OrderArchived,
	OrderCancelReplacePartiallyFailed,
	OrderCancelReplaceFailed,

	// Unknown error code
	Other(i32),
}

impl From<i32> for BinanceErrorCode {
	fn from(code: i32) -> Self {
		match code {
			-1000 => Self::Unknown,
			-1001 => Self::Disconnected,
			-1002 => Self::Unauthorized,
			-1003 => Self::TooManyRequests,
			-1006 => Self::UnexpectedResponse,
			-1007 => Self::Timeout,
			-1008 => Self::ServerBusy,
			-1013 => Self::InvalidMessage,
			-1014 => Self::UnknownOrderComposition,
			-1015 => Self::TooManyOrders,
			-1016 => Self::ServiceShuttingDown,
			-1020 => Self::UnsupportedOperation,
			-1021 => Self::InvalidTimestamp,
			-1022 => Self::InvalidSignature,

			-1100 => Self::IllegalChars,
			-1101 => Self::TooManyParameters,
			-1102 => Self::MandatoryParamEmptyOrMalformed,
			-1103 => Self::UnknownParam,
			-1104 => Self::UnreadParameters,
			-1105 => Self::ParamEmpty,
			-1106 => Self::ParamNotRequired,
			-1108 => Self::ParamOverflow,
			-1111 => Self::BadPrecision,
			-1112 => Self::NoDepth,
			-1114 => Self::TifNotRequired,
			-1115 => Self::InvalidTif,
			-1116 => Self::InvalidOrderType,
			-1117 => Self::InvalidSide,
			-1118 => Self::EmptyNewClOrdId,
			-1119 => Self::EmptyOrgClOrdId,
			-1120 => Self::BadInterval,
			-1121 => Self::BadSymbol,
			-1122 => Self::InvalidSymbolStatus,
			-1125 => Self::InvalidListenKey,
			-1127 => Self::MoreThanXXHours,
			-1128 => Self::OptionalParamsBadCombo,
			-1130 => Self::InvalidParameter,
			-1134 => Self::BadStrategyType,
			-1135 => Self::InvalidJson,
			-1139 => Self::InvalidTickerType,
			-1145 => Self::InvalidCancelRestrictions,
			-1151 => Self::DuplicateSymbols,
			-1152 => Self::InvalidSbeHeader,
			-1153 => Self::UnsupportedSchemaId,
			-1155 => Self::SbeDisabled,
			-1158 => Self::OcoOrderTypeRejected,
			-1160 => Self::OcoIcebergqtyTimeinforce,
			-1161 => Self::DeprecatedSchema,
			-1165 => Self::BuyOcoLimitMustBeBelow,
			-1166 => Self::SellOcoLimitMustBeAbove,
			-1168 => Self::BothOcoOrdersCannotBeLimit,
			-1169 => Self::InvalidTagNumber,
			-1170 => Self::TagNotDefinedInMessage,
			-1171 => Self::TagAppearsMoreThanOnce,
			-1172 => Self::TagOutOfOrder,
			-1173 => Self::GroupFieldsOutOfOrder,
			-1174 => Self::InvalidComponent,
			-1175 => Self::ResetSeqNumSupport,
			-1176 => Self::AlreadyLoggedIn,
			-1177 => Self::GarbledMessage,
			-1178 => Self::BadSenderCompid,
			-1179 => Self::BadSeqNum,
			-1180 => Self::ExpectedLogon,
			-1181 => Self::TooManyMessages,
			-1182 => Self::ParamsBadCombo,
			-1183 => Self::NotAllowedInDropCopySessions,
			-1184 => Self::DropCopySessionNotAllowed,
			-1185 => Self::DropCopySessionRequired,
			-1186 => Self::NotAllowedInOrderEntrySessions,
			-1187 => Self::NotAllowedInMarketDataSessions,
			-1188 => Self::IncorrectNumInGroupCount,
			-1189 => Self::DuplicateEntriesInAGroup,
			-1190 => Self::InvalidRequestId,
			-1191 => Self::TooManySubscriptions,
			-1196 => Self::BuyOcoStopLossMustBeAbove,
			-1197 => Self::SellOcoStopLossMustBeBelow,
			-1198 => Self::BuyOcoTakeProfitMustBeBelow,
			-1199 => Self::SellOcoTakeProfitMustBeAbove,

			-2010 => Self::NewOrderRejected,
			-2011 => Self::CancelRejected,
			-2013 => Self::NoSuchOrder,
			-2014 => Self::BadApiKeyFmt,
			-2015 => Self::RejectedMbxKey,
			-2016 => Self::NoTradingWindow,
			-2021 => Self::OrderCancelReplacePartiallyFailed,
			-2022 => Self::OrderCancelReplaceFailed,
			-2026 => Self::OrderArchived,

			code => Self::Other(code),
		}
	}
}
