// A module for communicating with the [Binance API](https://binance-docs.github.io/apidocs/spot/en/).

use std::{marker::PhantomData, str::FromStr, time::SystemTime};

use chrono::{Duration, Utc};
use generics::{
	http::{ApiError, BuildError, HandleError, *},
	reqwest::Url,
	ws::{WsConfig, WsHandler},
};
use hmac::{Hmac, Mac};
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::Sha256;
use v_utils::prelude::*;

use crate::traits::*;

// https://binance-docs.github.io/apidocs/spot/en/#general-api-information
impl<B, R> RequestHandler<B> for BinanceRequestHandler<'_, R>
where
	B: Serialize,
	R: DeserializeOwned,
{
	type Successful = R;

	fn base_url(&self, is_test: bool) -> String {
		match is_test {
			true => self.options.http_url.as_str_test().unwrap().to_owned(),
			false => self.options.http_url.as_str().to_owned(),
		}
	}

	#[tracing::instrument(skip_all, fields(?builder))]
	fn build_request(&self, mut builder: RequestBuilder, request_body: &Option<B>, _: u8) -> Result<Request, BuildError> {
		if let Some(body) = request_body {
			let encoded = serde_urlencoded::to_string(body)?;
			builder = builder.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(encoded);
		}

		if self.options.http_auth != BinanceAuth::None {
			// https://binance-docs.github.io/apidocs/spot/en/#signed-trade-user_data-and-margin-endpoint-security
			let pubkey = self.options.pubkey.as_deref().ok_or(AuthError::MissingApiKey)?;
			builder = builder.header("X-MBX-APIKEY", pubkey);

			if self.options.http_auth == BinanceAuth::Sign {
				let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap(); // always after the epoch
				let timestamp = time.as_millis();

				builder = builder.query(&[("timestamp", timestamp)]);
				if let Some(recv_window) = self.options.recv_window {
					builder = builder.query(&[("recvWindow", recv_window)]);
				}

				let secret = self.options.secret.as_ref().map(|s| s.expose_secret()).ok_or(AuthError::MissingSecret)?;
				let mut hmac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap(); // hmac accepts key of any length

				let mut request = builder.build().expect("From what I understand, can't trigger this from client-side");
				let query = request.url().query().unwrap();
				let body = request.body().and_then(|body| body.as_bytes()).unwrap_or_default();

				hmac.update(&[query.as_bytes(), body].concat());
				let signature = hex::encode(hmac.finalize().into_bytes());

				request.url_mut().query_pairs_mut().append_pair("signature", &signature);

				return Ok(request);
			}
		}
		Ok(builder.build().expect("don't expect this to be reached by client, so fail fast for dev"))
	}

	fn handle_response(&self, status: StatusCode, headers: HeaderMap, response_body: Bytes) -> Result<Self::Successful, HandleError> {
		if status.is_success() {
			serde_json::from_slice(&response_body).map_err(|error| {
				tracing::debug!("Failed to parse response due to an error: {}", error);
				HandleError::Parse(error)
			})
		} else {
			// https://binance-docs.github.io/apidocs/spot/en/#limits

			//TODO; act on error-codes
			if status == 429 || status == 418 {
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
					_ => eyre!("Could't interpret Retry-After header").into(),
				};
				return Err(e);
			}

			let e: BinanceError = match serde_json::from_slice::<BinanceError>(&response_body) {
				Ok(binance_error) => binance_error,
				Err(parse_error) => return Err(HandleError::Parse(parse_error)),
			};
			Err(ApiError::from(e).into())
		}
	}
}

// Ws {{{
#[derive(Clone, Debug, derive_new::new)]
pub struct BinanceWsHandler {
	options: BinanceOptions,
}
impl WsHandler for BinanceWsHandler {
	#[inline(always)]
	fn ws_config(&self) -> WsConfig {
		let mut config = self.options.ws_config.clone();
		if self.options.ws_url != BinanceWsUrl::None {
			config.base_url = Some(self.options.ws_url.to_owned());
		}
		config
	}
}
impl WsOption for BinanceOption {
	type WsHandler = BinanceWsHandler;

	#[inline(always)]
	fn ws_handler(options: Self::Options) -> Self::WsHandler {
		BinanceWsHandler::new(options)
	}
}
//,}}}

/// Options that can be set when creating handlers
#[derive(Debug, Default)]
pub enum BinanceOption {
	#[default]
	None,
	/// API key
	Pubkey(String),
	/// Api secret
	Secret(SecretString),

	/// Number of milliseconds the request is valid for. Only applicable for signed requests.
	RecvWindow(u16),
	/// Base url for HTTP requests
	HttpUrl(BinanceHttpUrl),
	/// Authentication type for HTTP requests
	HttpAuth(BinanceAuth),

	/// Base url for WebSocket connections
	WsUrl(BinanceWsUrl),
	/// [WebSocketConfig] used for creating [WebSocketConnection]s
	/// `url_prefix` will be overridden by [WebSocketUrl](Self::WebSocketUrl) unless `WebSocketUrl` is [BinanceWebSocketUrl::None].
	WsConfig(WsConfig),
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
	/// `https://data.binance.com`
	SpotData,
	/// `https://fapi.binance.com`
	FuturesUsdM,
	/// `https://dapi.binance.com`
	FuturesCoinM,
	/// `https://eapi.binance.com`
	EuropeanOptions,
	/// The url will not be modified by [BinanceRequestHandler]
	#[default]
	None,
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
			Self::SpotData => "https://data.binance.com",
			Self::FuturesUsdM => "https://fapi.binance.com",
			Self::FuturesCoinM => "https://dapi.binance.com",
			Self::EuropeanOptions => "https://eapi.binance.com",
			Self::None => "",
		}
	}

	//TODO: impl more cleanly
	#[inline(always)]
	fn as_str_test(&self) -> Option<&'static str> {
		match self {
			Self::Spot => Some("https://testnet.binance.vision"),
			Self::Spot1 => Some("https://testnet.binance.vision"),
			Self::Spot2 => Some("https://testnet.binance.vision"),
			Self::Spot3 => Some("https://testnet.binance.vision"),
			Self::Spot4 => Some("https://testnet.binance.vision"),
			Self::SpotData => Some("https://testnet.binance.vision"),
			Self::FuturesUsdM => Some("https://testnet.binancefuture.com"),
			Self::FuturesCoinM => Some("https://testnet.binancefuture.com"),
			Self::EuropeanOptions => None,
			Self::None => Some(""),
		}
	}
}

/// A `enum` that represents the base url of the Binance WebSocket API
#[derive(Debug, Eq, PartialEq, Clone, Copy, Default)]
#[non_exhaustive]
pub enum BinanceWsUrl {
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
impl BinanceWsUrl {
	// Can't impl [ToOwned], as there is a blanket impl of it on everything with [Clone]
	fn to_owned(&self) -> Url {
		match self {
			Self::Spot9443 => Url::parse("wss://stream.binance.com:9443").unwrap(),
			Self::Spot443 => Url::parse("wss://stream.binance.com:443").unwrap(),
			Self::SpotTest => Url::parse("wss://testnet.binance.vision").unwrap(),
			Self::SpotData => Url::parse("wss://data-stream.binance.com").unwrap(),
			Self::WebSocket443 => Url::parse("wss://ws-api.binance.com:443").unwrap(),
			Self::WebSocket9443 => Url::parse("wss://ws-api.binance.com:9443").unwrap(),
			Self::FuturesUsdM => Url::parse("wss://fstream.binance.com").unwrap(),
			Self::FuturesUsdMAuth => Url::parse("wss://fstream-auth.binance.com").unwrap(),
			Self::FuturesCoinM => Url::parse("wss://dstream.binance.com").unwrap(),
			Self::FuturesUsdMTest => Url::parse("wss://stream.binancefuture.com").unwrap(),
			Self::FuturesCoinMTest => Url::parse("wss://dstream.binancefuture.com").unwrap(),
			Self::EuropeanOptions => Url::parse("wss://nbstream.binance.com").unwrap(),
			Self::None => panic!("calling .to_owned() on BinanceWsUrl::None is invalid"),
		}
	}
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Default)]
pub enum BinanceAuth {
	Sign,
	Key, //Q: Not sure if anything uses it.
	#[default]
	None,
}

#[derive(Debug)]
pub enum BinanceHandlerError {
	ApiError(BinanceError),
	RateLimitError { retry_after: Option<u32> },
	ParseError,
}

/// A `struct` that implements [RequestHandler]
pub struct BinanceRequestHandler<'a, R: DeserializeOwned> {
	options: BinanceOptions,
	_phantom: PhantomData<&'a R>,
}

/// A `struct` that represents a set of [BinanceOption] s.
#[derive(Clone, derive_more::Debug)]
pub struct BinanceOptions {
	/// see [BinanceOption::Key]
	pub pubkey: Option<String>,
	/// see [BinanceOption::Secret]
	#[debug("[REDACTED]")]
	pub secret: Option<SecretString>,
	// see [BinanceOption::RecvWindow]
	pub recv_window: Option<u16>,
	/// see [BinanceOption::HttpUrl]
	pub http_url: BinanceHttpUrl,
	/// see [BinanceOption::HttpAuth]
	pub http_auth: BinanceAuth,
	/// see [BinanceOption::WsUrl]
	pub ws_url: BinanceWsUrl,
	/// see [BinanceOption::WsConfig]
	pub ws_config: WsConfig,
	/// see [BinanceOption::Test]
	pub test: bool,
}
impl Default for BinanceOptions {
	fn default() -> Self {
		let ws_config = WsConfig {
			refresh_after: std::time::Duration::from_hours(12),
			..Default::default()
		};
		Self {
			pubkey: None,
			secret: None,
			recv_window: None,
			http_url: Default::default(),
			http_auth: Default::default(),
			ws_url: Default::default(),
			ws_config,
			test: false,
		}
	}
}
impl HandlerOptions for BinanceOptions {
	type OptionItem = BinanceOption;

	fn update(&mut self, option: Self::OptionItem) {
		match option {
			Self::OptionItem::None => (),
			Self::OptionItem::Pubkey(v) => self.pubkey = Some(v),
			Self::OptionItem::RecvWindow(v) => self.recv_window = Some(v),
			Self::OptionItem::Secret(v) => self.secret = Some(v),
			Self::OptionItem::HttpUrl(v) => self.http_url = v,
			Self::OptionItem::HttpAuth(v) => self.http_auth = v,
			Self::OptionItem::WsUrl(v) => self.ws_url = v,
			Self::OptionItem::WsConfig(v) => self.ws_config = v,
		}
	}

	fn is_authenticated(&self) -> bool {
		self.pubkey.is_some() // some end points are satisfied with just the key, and it's really difficult to provide only a key without a secret from the clientside, so assume intent if it's missing.
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

impl HandlerOption for BinanceOption {
	type Options = BinanceOptions;
}

// Error Codes {{{
#[derive(Clone, Debug, Deserialize)]
pub struct BinanceError {
	pub code: BinanceErrorCode,
	pub msg: String,
}
impl From<BinanceError> for ApiError {
	fn from(e: BinanceError) -> Self {
		//HACK
		eyre!("Binance API error: {}", e.msg).into()
	}
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(from = "i32")]
pub enum BinanceErrorCode {
	// 10xx - General Server/Network
	Unknown(i32),
	Disconnected(i32),
	Unauthorized(i32),
	TooManyRequests(i32),
	UnexpectedResponse(i32),
	Timeout(i32),
	ServerBusy(i32),
	InvalidMessage(i32),
	UnknownOrderComposition(i32),
	TooManyOrders(i32),
	ServiceShuttingDown(i32),
	UnsupportedOperation(i32),
	InvalidTimestamp(i32),
	InvalidSignature(i32),

	// 11xx - Request issues
	IllegalChars(i32),
	TooManyParameters(i32),
	MandatoryParamEmptyOrMalformed(i32),
	UnknownParam(i32),
	UnreadParameters(i32),
	ParamEmpty(i32),
	ParamNotRequired(i32),
	ParamOverflow(i32),
	BadPrecision(i32),
	NoDepth(i32),
	TifNotRequired(i32),
	InvalidTif(i32),
	InvalidOrderType(i32),
	InvalidSide(i32),
	EmptyNewClOrdId(i32),
	EmptyOrgClOrdId(i32),
	BadInterval(i32),
	BadSymbol(i32),
	InvalidSymbolStatus(i32),
	InvalidListenKey(i32),
	MoreThanXXHours(i32),
	OptionalParamsBadCombo(i32),
	InvalidParameter(i32),
	BadStrategyType(i32),
	InvalidJson(i32),
	InvalidTickerType(i32),
	InvalidCancelRestrictions(i32),
	DuplicateSymbols(i32),
	InvalidSbeHeader(i32),
	UnsupportedSchemaId(i32),
	SbeDisabled(i32),
	OcoOrderTypeRejected(i32),
	OcoIcebergqtyTimeinforce(i32),
	DeprecatedSchema(i32),
	BuyOcoLimitMustBeBelow(i32),
	SellOcoLimitMustBeAbove(i32),
	BothOcoOrdersCannotBeLimit(i32),
	InvalidTagNumber(i32),
	TagNotDefinedInMessage(i32),
	TagAppearsMoreThanOnce(i32),
	TagOutOfOrder(i32),
	GroupFieldsOutOfOrder(i32),
	InvalidComponent(i32),
	ResetSeqNumSupport(i32),
	AlreadyLoggedIn(i32),
	GarbledMessage(i32),
	BadSenderCompid(i32),
	BadSeqNum(i32),
	ExpectedLogon(i32),
	TooManyMessages(i32),
	ParamsBadCombo(i32),
	NotAllowedInDropCopySessions(i32),
	DropCopySessionNotAllowed(i32),
	DropCopySessionRequired(i32),
	NotAllowedInOrderEntrySessions(i32),
	NotAllowedInMarketDataSessions(i32),
	IncorrectNumInGroupCount(i32),
	DuplicateEntriesInAGroup(i32),
	InvalidRequestId(i32),
	TooManySubscriptions(i32),
	BuyOcoStopLossMustBeAbove(i32),
	SellOcoStopLossMustBeBelow(i32),
	BuyOcoTakeProfitMustBeBelow(i32),
	SellOcoTakeProfitMustBeAbove(i32),

	// 20xx - Business logic errors
	NewOrderRejected(i32),
	CancelRejected(i32),
	NoSuchOrder(i32),
	BadApiKeyFmt(i32),
	RejectedMbxKey(i32),
	NoTradingWindow(i32),
	OrderArchived(i32),
	OrderCancelReplacePartiallyFailed(i32),
	OrderCancelReplaceFailed(i32),

	// Unknown error code
	Other(i32),
}

impl From<i32> for BinanceErrorCode {
	fn from(code: i32) -> Self {
		match code {
			-1000 => Self::Unknown(code),
			-1001 => Self::Disconnected(code),
			-1002 => Self::Unauthorized(code),
			-1003 => Self::TooManyRequests(code),
			-1006 => Self::UnexpectedResponse(code),
			-1007 => Self::Timeout(code),
			-1008 => Self::ServerBusy(code),
			-1013 => Self::InvalidMessage(code),
			-1014 => Self::UnknownOrderComposition(code),
			-1015 => Self::TooManyOrders(code),
			-1016 => Self::ServiceShuttingDown(code),
			-1020 => Self::UnsupportedOperation(code),
			-1021 => Self::InvalidTimestamp(code),
			-1022 => Self::InvalidSignature(code),

			-1100 => Self::IllegalChars(code),
			-1101 => Self::TooManyParameters(code),
			-1102 => Self::MandatoryParamEmptyOrMalformed(code),
			-1103 => Self::UnknownParam(code),
			-1104 => Self::UnreadParameters(code),
			-1105 => Self::ParamEmpty(code),
			-1106 => Self::ParamNotRequired(code),
			-1108 => Self::ParamOverflow(code),
			-1111 => Self::BadPrecision(code),
			-1112 => Self::NoDepth(code),
			-1114 => Self::TifNotRequired(code),
			-1115 => Self::InvalidTif(code),
			-1116 => Self::InvalidOrderType(code),
			-1117 => Self::InvalidSide(code),
			-1118 => Self::EmptyNewClOrdId(code),
			-1119 => Self::EmptyOrgClOrdId(code),
			-1120 => Self::BadInterval(code),
			-1121 => Self::BadSymbol(code),
			-1122 => Self::InvalidSymbolStatus(code),
			-1125 => Self::InvalidListenKey(code),
			-1127 => Self::MoreThanXXHours(code),
			-1128 => Self::OptionalParamsBadCombo(code),
			-1130 => Self::InvalidParameter(code),
			-1134 => Self::BadStrategyType(code),
			-1135 => Self::InvalidJson(code),
			-1139 => Self::InvalidTickerType(code),
			-1145 => Self::InvalidCancelRestrictions(code),
			-1151 => Self::DuplicateSymbols(code),
			-1152 => Self::InvalidSbeHeader(code),
			-1153 => Self::UnsupportedSchemaId(code),
			-1155 => Self::SbeDisabled(code),
			-1158 => Self::OcoOrderTypeRejected(code),
			-1160 => Self::OcoIcebergqtyTimeinforce(code),
			-1161 => Self::DeprecatedSchema(code),
			-1165 => Self::BuyOcoLimitMustBeBelow(code),
			-1166 => Self::SellOcoLimitMustBeAbove(code),
			-1168 => Self::BothOcoOrdersCannotBeLimit(code),
			-1169 => Self::InvalidTagNumber(code),
			-1170 => Self::TagNotDefinedInMessage(code),
			-1171 => Self::TagAppearsMoreThanOnce(code),
			-1172 => Self::TagOutOfOrder(code),
			-1173 => Self::GroupFieldsOutOfOrder(code),
			-1174 => Self::InvalidComponent(code),
			-1175 => Self::ResetSeqNumSupport(code),
			-1176 => Self::AlreadyLoggedIn(code),
			-1177 => Self::GarbledMessage(code),
			-1178 => Self::BadSenderCompid(code),
			-1179 => Self::BadSeqNum(code),
			-1180 => Self::ExpectedLogon(code),
			-1181 => Self::TooManyMessages(code),
			-1182 => Self::ParamsBadCombo(code),
			-1183 => Self::NotAllowedInDropCopySessions(code),
			-1184 => Self::DropCopySessionNotAllowed(code),
			-1185 => Self::DropCopySessionRequired(code),
			-1186 => Self::NotAllowedInOrderEntrySessions(code),
			-1187 => Self::NotAllowedInMarketDataSessions(code),
			-1188 => Self::IncorrectNumInGroupCount(code),
			-1189 => Self::DuplicateEntriesInAGroup(code),
			-1190 => Self::InvalidRequestId(code),
			-1191 => Self::TooManySubscriptions(code),
			-1196 => Self::BuyOcoStopLossMustBeAbove(code),
			-1197 => Self::SellOcoStopLossMustBeBelow(code),
			-1198 => Self::BuyOcoTakeProfitMustBeBelow(code),
			-1199 => Self::SellOcoTakeProfitMustBeAbove(code),

			-2010 => Self::NewOrderRejected(code),
			-2011 => Self::CancelRejected(code),
			-2013 => Self::NoSuchOrder(code),
			-2014 => Self::BadApiKeyFmt(code),
			-2015 => Self::RejectedMbxKey(code),
			-2016 => Self::NoTradingWindow(code),
			-2021 => Self::OrderCancelReplacePartiallyFailed(code),
			-2022 => Self::OrderCancelReplaceFailed(code),
			-2026 => Self::OrderArchived(code),

			code => {
				warn!("Encountered unknown Binance error code: {code}");
				Self::Other(code)
			}
		}
	}
}
//,}}}
