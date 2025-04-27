// A module for communicating with the [Binance API](https://binance-docs.github.io/apidocs/spot/en/).

use std::{collections::HashSet, marker::PhantomData, str::FromStr, time::SystemTime};

use chrono::{DateTime, Utc};
use eyre::eyre;
use generics::{
	AuthError, UrlError,
	http::{ApiError, BuildError, HandleError, *},
	tokio_tungstenite::tungstenite,
	ws::{ContentEvent, ResponseOrContent, Topic, WsConfig, WsError, WsHandler},
};
use hmac::{Hmac, Mac};
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::Sha256;
use url::Url;

use crate::traits::*;

// https://binance-docs.github.io/apidocs/spot/en/#general-api-information
impl<B, R> RequestHandler<B> for BinanceRequestHandler<'_, R>
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
		if let Some(body) = request_body {
			let encoded = serde_urlencoded::to_string(body)?;
			builder = builder.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(encoded);
		}

		if self.options.http_auth != BinanceAuth::None {
			// https://binance-docs.github.io/apidocs/spot/en/#signed-trade-user_data-and-margin-endpoint-security
			let pubkey = self.options.pubkey.as_deref().ok_or(AuthError::MissingPubkey)?;
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
						let until = Some(Utc::now() + chrono::Duration::seconds(s as i64));
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

// Ws stuff {{{
#[derive(Clone, Debug)]
pub struct BinanceWsHandler {
	options: BinanceOptions,
	/// Binance has a retarded `listen-key` system. This is needed only for that.
	last_keep_alive: SystemTime,
}
impl BinanceWsHandler {
	pub fn new(options: BinanceOptions) -> Self {
		Self {
			options,
			last_keep_alive: SystemTime::UNIX_EPOCH, // semantically creation itself does nothing for refreshing the token. But refreshment timer on it will be set to 0 on creation, so that's when we'll set it to [now](SystemTime::now)
		}
	}
}
impl WsHandler for BinanceWsHandler {
	fn config(&self) -> Result<WsConfig, UrlError> {
		let mut config = self.options.ws_config.clone();
		match self.options.ws_url {
			BinanceWsUrl::None => tracing::warn!(
				"BinanceWsUrl was not set. Due to Binance shenanigans, any provided topics will now be ignored, and must be manually hardcoded into the provided url on creation of the websocket. However, recommended approach is to simply provide a BinanceOption::WsUrl."
			),
			_ => {
				let mut streams = String::new();
				config.topics = config.topics.union(&self.options.ws_topics).cloned().collect();
				for (i, topic) in config.topics.iter().enumerate() {
					if i > 0 {
						streams.push('/');
					}
					streams.push_str(topic);
				}
				let base_url = match self.options.test {
					true => self.options.ws_url.url_testnet().ok_or_else(|| UrlError::MissingTestnet(self.options.ws_url.url_mainnet()))?,
					false => self.options.ws_url.url_mainnet(),
				};
				config.base_url = Some(base_url.join(&format!("stream?streams={streams}")).unwrap());
			}
		}
		Ok(config)
	}

	fn handle_auth(&mut self) -> Result<Vec<tungstenite::Message>, WsError> {
		if self.options.ws_config.auth {
			//TODO: implement ws auth once I can acquire ed25519 keys: https://developers.binance.com/docs/derivatives/usds-margined-futures/websocket-api-general-info#log-in-with-api-key-signed

			let pubkey = self.options.pubkey.as_ref().ok_or(AuthError::MissingPubkey)?;
			let secret = self.options.secret.as_ref().ok_or(AuthError::MissingSecret)?;

			//TODO:
			/*
			match
				user_data_stream => POST /api/v3/userDataStream
				trade => need to sign each request (can't sign connection itself without ed25519), so do nothing here
			*/
		}

		Ok(vec![])
	}

	fn handle_subscribe(&mut self, topics: HashSet<Topic>) -> eyre::Result<Vec<tungstenite::Message>, WsError> {
		topics
			.into_iter()
			.map(|topic| {
				let topic = match topic {
					Topic::Trade(topic) => topic,
					_ => return Err(WsError::Subscription("Binance only supports string topics".to_owned())),
				};
				todo!();
			})
			.collect::<Result<Vec<_>, _>>()
	}

	fn handle_jrpc(&mut self, jrpc: serde_json::Value) -> Result<ResponseOrContent, WsError> {
		//TODO: handle listen key expiration \
		//match jrpc["e"].as_str().expect("missing event type") { // matches with event_type
		//	"listenKeyExpired" => todo!(),
		//	_ => Ok(None),
		//}
		#[derive(serde::Deserialize)]
		struct NamedStreamData {
			pub stream: String,
			pub data: serde_json::Value,
		}
		let (event_topic, data) = {
			match serde_json::from_value::<NamedStreamData>(jrpc.clone()) {
				Ok(NamedStreamData { stream, data }) => (stream, data),
				Err(_) => ("".to_string(), jrpc),
			}
		};
		assert!(data.is_object(), "data should be an object");

		let (event_type, event_time, event_data) = {
			//dbg: dirty impl
			let mut event_data = data.as_object().unwrap().to_owned();
			let event_type = data["e"].as_str().unwrap().to_owned();
			event_data.remove("e");
			let event_ts: i64 = data["E"].as_i64().unwrap();
			dbg!(&event_ts);
			let event_time = DateTime::<Utc>::from_timestamp_millis(event_ts).unwrap();
			event_data.remove("E");
			(event_type, event_time, event_data.into())
		};

		let content = ContentEvent {
			data: event_data,
			topic: event_topic,
			time: event_time,
			event_type,
		};
		Ok(ResponseOrContent::Content(content)) //dbg
	}

	// stream listen-key keepalive works for:
	// - [x] binance spot
	// - [?] binance perp

	//	fn handle_post(&mut self) -> Result<Option<Vec<tungstenite::Message>>, WsError> {
	//	if SystemTime::now().duration_since(self.last_keep_alive).unwrap() > Duration::from_mins(30) {
	//		//XXX: will fail if it's not a USER_DATA_STREAM //TODO: generalize to all binance streams
	//		let msg_json = serde_json::json!({
	//			"id": "815d5fce-0880-4287-a567-80badf004c74",
	//			"method": "userDataStream.ping",
	//			"params": {
	//				"apiKey": self.options.pubkey.as_ref().unwrap()
	//			}
	//		});
	//		return Ok(Some(vec![tungstenite::Message::Text(msg_json.to_string().into())]));
	//	}
	//	Ok(None)
	//}
	//if SystemTime::now().duration_since(self.last_keep_alive).unwrap() > Duration::from_mins(30) {
	//	//XXX: will fail if it's not a USER_DATA_STREAM
	//	//TODO send `PUT /api/v3/userDataStream`
	//	let client = crate::Client::default();
	//	.request(
	//		&self.options,
	//		"PUT",
	//		"/api/v3/userDataStream",
	//		None::<()>,
	//	)
	//}
}
impl WsOption for BinanceOption {
	type WsHandler = BinanceWsHandler;

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
	/// Use testnet
	Test(bool),

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
	/// See [WsConfig::topics]. Will be merged with those manually defined in [Self::WsConfig::topics], if any.
	WsTopics(Vec<String>),
}

/// A `enum` that represents the base url of the Binance REST API.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
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
impl EndpointUrl for BinanceHttpUrl {
	fn url_mainnet(&self) -> Url {
		match self {
			Self::Spot => Url::parse("https://api.binance.com").unwrap(),
			Self::Spot1 => Url::parse("https://api1.binance.com").unwrap(),
			Self::Spot2 => Url::parse("https://api2.binance.com").unwrap(),
			Self::Spot3 => Url::parse("https://api3.binance.com").unwrap(),
			Self::Spot4 => Url::parse("https://api4.binance.com").unwrap(),
			Self::SpotData => Url::parse("https://data.binance.com").unwrap(),
			Self::FuturesUsdM => Url::parse("https://fapi.binance.com").unwrap(),
			Self::FuturesCoinM => Url::parse("https://dapi.binance.com").unwrap(),
			Self::EuropeanOptions => Url::parse("https://eapi.binance.com").unwrap(),
			Self::None => Url::parse("").unwrap(),
		}
	}

	fn url_testnet(&self) -> Option<Url> {
		match self {
			Self::Spot => Some(Url::parse("https://testnet.binance.vision").unwrap()),
			Self::Spot1 => Some(Url::parse("https://testnet.binance.vision").unwrap()),
			Self::Spot2 => Some(Url::parse("https://testnet.binance.vision").unwrap()),
			Self::Spot3 => Some(Url::parse("https://testnet.binance.vision").unwrap()),
			Self::Spot4 => Some(Url::parse("https://testnet.binance.vision").unwrap()),
			Self::SpotData => Some(Url::parse("https://testnet.binance.vision").unwrap()),
			Self::FuturesUsdM => Some(Url::parse("https://testnet.binancefuture.com").unwrap()),
			Self::FuturesCoinM => Some(Url::parse("https://testnet.binancefuture.com").unwrap()),
			Self::EuropeanOptions => None,
			Self::None => Some(Url::parse("").unwrap()),
		}
	}
}

/// A `enum` that represents the base url of the Binance WebSocket API
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BinanceWsUrl {
	/// Evaluated to whatever spot url is estimated to be currently preferrable.
	Spot,
	/// `wss://stream.binance.com:9443`
	Spot9443,
	/// `wss://stream.binance.com:443`
	Spot443,
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
	/// `wss://nbstream.binance.com`
	EuropeanOptions,
	/// The url will not be modified by [BinanceRequestHandler]
	#[default]
	None,
}
impl EndpointUrl for BinanceWsUrl {
	// Can't impl [ToOwned], as there is a blanket impl of it on everything with [Clone]
	fn url_mainnet(&self) -> url::Url {
		match self {
			Self::Spot => Url::parse("wss://stream.binance.com:9443").unwrap(), //TODO: actually have some metric to select the best url here
			Self::Spot9443 => Url::parse("wss://stream.binance.com:9443").unwrap(),
			Self::Spot443 => Url::parse("wss://stream.binance.com:443").unwrap(),
			Self::SpotData => Url::parse("wss://data-stream.binance.com").unwrap(),
			Self::WebSocket443 => Url::parse("wss://ws-api.binance.com:443").unwrap(),
			Self::WebSocket9443 => Url::parse("wss://ws-api.binance.com:9443").unwrap(),
			Self::FuturesUsdM => Url::parse("wss://fstream.binance.com").unwrap(),
			Self::FuturesUsdMAuth => Url::parse("wss://fstream-auth.binance.com").unwrap(),
			Self::FuturesCoinM => Url::parse("wss://dstream.binance.com").unwrap(),
			Self::EuropeanOptions => Url::parse("wss://nbstream.binance.com").unwrap(),
			Self::None => Url::parse("").unwrap(),
		}
	}

	fn url_testnet(&self) -> Option<url::Url> {
		match self {
			Self::Spot => Some(Url::parse("wss://testnet.binance.vision").unwrap()),
			Self::Spot9443 => Some(Url::parse("wss://testnet.binance.vision:9443").unwrap()),
			Self::Spot443 => Some(Url::parse("wss://testnet.binance.vision:443").unwrap()),
			Self::FuturesUsdM => Some(Url::parse("wss://stream.binancefuture.com").unwrap()),
			Self::FuturesCoinM => Some(Url::parse("wss://dstream.binancefuture.com").unwrap()),
			Self::SpotData | Self::WebSocket443 | Self::WebSocket9443 | Self::FuturesUsdMAuth | Self::EuropeanOptions | Self::None => None,
		}
	}
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
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
#[derive(Clone, derive_more::Debug, Default)]
pub struct BinanceOptions {
	/// see [BinanceOption::Pubkey]
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
	/// see [BinanceOption::WsTopics]
	pub ws_topics: HashSet<String>,
	/// see [BinanceOption::Test]
	pub test: bool,
}
impl HandlerOptions for BinanceOptions {
	type OptionItem = BinanceOption;

	fn update(&mut self, option: Self::OptionItem) {
		match option {
			Self::OptionItem::None => (),
			Self::OptionItem::Pubkey(v) => self.pubkey = Some(v),
			Self::OptionItem::RecvWindow(v) => self.recv_window = Some(v),
			Self::OptionItem::Test(v) => self.test = v,
			Self::OptionItem::Secret(v) => self.secret = Some(v),
			Self::OptionItem::HttpUrl(v) => self.http_url = v,
			Self::OptionItem::HttpAuth(v) => self.http_auth = v,
			Self::OptionItem::WsUrl(v) => self.ws_url = v,
			Self::OptionItem::WsConfig(v) => self.ws_config = v,
			Self::OptionItem::WsTopics(v) => self.ws_topics = v.into_iter().collect(),
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

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
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
				tracing::warn!("Encountered unknown Binance error code: {code}");
				Self::Other(code)
			}
		}
	}
}
//,}}}
