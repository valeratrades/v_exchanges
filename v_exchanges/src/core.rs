use std::collections::{BTreeMap, VecDeque};

use adapters::{Client, generics::http::RequestError};
use chrono::{DateTime, TimeDelta, Utc};
use derive_more::{Deref, DerefMut};
use secrecy::SecretString;
use serde_json::json;
use v_utils::{
	prelude::*,
	trades::{Asset, Kline, Pair, Timeframe, Usd},
	utils::filter_nulls,
};

/// Main trait for all standardized exchange interactions
/// # Other
/// - each private method provides recv_window
#[async_trait::async_trait]
pub trait Exchange: std::fmt::Debug + Send + Sync {
	// dev {{{
	/// will always be `Some` when created from `AbsMarket`. When creating client manually could lead to weird errors from this method being used elsewhere, like displaying a `AbsMarket` object.
	fn source_market(&self) -> AbsMarket;
	fn exchange_name(&self) -> &'static str {
		self.source_market().exchange_name()
	}
	#[doc(hidden)]
	fn __client_mut(&mut self) -> &mut Client;
	#[doc(hidden)]
	fn __client(&self) -> &Client;
	//,}}}

	// Config {{{
	fn auth(&mut self, key: String, secret: SecretString);
	/// Set number of **milliseconds** the request is valid for. Recv Window of over a minute does not make sense, thus it's expressed as u16.
	//Q: really don't think this should be used like that, globally, - if unable to find cases, rm in a month (now is 2025/01/24)
	#[deprecated(note = "This shouldn't be a global setting, but a per-request one. Use `recv_window` in the request instead.")]
	fn set_recv_window(&mut self, recv_window: u16);
	fn set_timeout(&mut self, timeout: std::time::Duration) {
		self.__client_mut().client.config.timeout = timeout;
	}
	fn set_retry_cooldown(&mut self, cooldown: std::time::Duration) {
		self.__client_mut().client.config.retry_cooldown = cooldown;
	}
	fn set_max_tries(&mut self, max: u8) {
		self.__client_mut().client.config.max_tries = max;
	}
	//DO: same for other fields in [RequestConfig](v_exchanges_api_generics::http::RequestConfig)
	//,}}}

	async fn exchange_info(&self, m: AbsMarket) -> ExchangeResult<ExchangeInfo>;

	//? should I have Self::Pair too? Like to catch the non-existent ones immediately? Although this would increase the error surface on new listings.
	async fn klines(&self, pair: Pair, tf: Timeframe, range: RequestRange, m: AbsMarket) -> ExchangeResult<Klines>;

	/// If no pairs are specified, returns for all;
	async fn prices(&self, pairs: Option<Vec<Pair>>, m: AbsMarket) -> ExchangeResult<BTreeMap<Pair, f64>>;
	async fn price(&self, pair: Pair, m: AbsMarket) -> ExchangeResult<f64>;

	// Defined in terms of actors
	//TODO!!!: async fn spawn_klines_listener(&self, symbol: Pair, tf: Timeframe) -> mpsc::Receiver<Kline>;

	/// balance of a specific asset. Does not guarantee provision of USD values.
	async fn asset_balance(&self, asset: Asset, recv_window: Option<u16>, m: AbsMarket) -> ExchangeResult<AssetBalance>;
	/// vec of _non-zero_ balances exclusively. Provides USD values.
	async fn balances(&self, recv_window: Option<u16>, m: AbsMarket) -> ExchangeResult<Balances>;
	//? potentially `total_balance`? Would return precompiled USDT-denominated balance of a (bybit::wallet/binance::account)
	// balances are defined for each margin type: [futures_balance, spot_balance, margin_balance], but note that on some exchanges, (like bybit), some of these may point to the same exact call
	// to negate confusion could add a `total_balance` endpoint

	//? could implement many things that are _explicitly_ combinatorial. I can imagine several cases, where knowing that say the specified limit for the klines is wayyy over the max and that you may be opting into a long wait by calling it, could be useful.
}
impl std::fmt::Display for dyn Exchange {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.exchange_name())
	}
}

// Exchange Error {{{
pub type ExchangeResult<T> = Result<T, ExchangeError>;
#[derive(Error, Debug, derive_more::Display, derive_more::From)]
pub enum ExchangeError {
	Request(RequestError),
	Exchange(WrongExchangeError),
	Timeframe(UnsupportedTimeframeError),
	Range(RequestRangeError),
	Other(Report),
}
#[derive(Error, Debug, derive_new::new)]
#[error("Chosen exchange does not support the requested timeframe. Provided: {provided}, allowed: {allowed:?}")]
pub struct UnsupportedTimeframeError {
	provided: Timeframe,
	allowed: Vec<Timeframe>,
}
//,}}}

// AbsMarket {{{
#[derive(derive_more::Debug, derive_new::new, thiserror::Error)]
pub struct WrongExchangeError {
	correct: &'static str,
	provided: AbsMarket,
}
impl std::fmt::Display for WrongExchangeError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"Wrong exchange provided. Accessible object: {:?}, provided \"abs market path\": {}",
			self.correct, self.provided
		)
	}
}

pub trait MarketTrait {
	fn client(&self, source_market: AbsMarket) -> Box<dyn Exchange>;
	fn client_authenticated(&self, key: String, secret: SecretString, source_market: AbsMarket) -> Box<dyn Exchange> {
		let mut client = self.client(source_market);
		client.auth(key, secret);
		client
	}
	fn abs_market(&self) -> AbsMarket;
}

#[derive(Debug, Clone, Copy)]
pub enum AbsMarket {
	#[cfg(feature = "binance")]
	Binance(crate::binance::Market),
	#[cfg(feature = "bybit")]
	Bybit(crate::bybit::Market),
	#[cfg(feature = "mexc")]
	Mexc(crate::mexc::Market),
}
impl AbsMarket {
	pub fn client(&self) -> Box<dyn Exchange> {
		match self {
			#[cfg(feature = "binance")]
			Self::Binance(m) => m.client(*self),
			#[cfg(feature = "bybit")]
			Self::Bybit(m) => m.client(*self),
			#[cfg(feature = "mexc")]
			Self::Mexc(m) => m.client(*self),
		}
	}

	//Q: more I think about it, more this seems redundant / stupid according to Tiger Style
	pub fn client_authenticated(&self, key: String, secret: SecretString) -> Box<dyn Exchange> {
		match self {
			#[cfg(feature = "binance")]
			Self::Binance(m) => m.client_authenticated(key, secret, *self),
			#[cfg(feature = "bybit")]
			Self::Bybit(m) => m.client_authenticated(key, secret, *self),
			#[cfg(feature = "mexc")]
			Self::Mexc(m) => m.client_authenticated(key, secret, *self),
		}
	}

	pub fn exchange_name(&self) -> &'static str {
		match self {
			#[cfg(feature = "binance")]
			Self::Binance(_) => "Binance",
			#[cfg(feature = "bybit")]
			Self::Bybit(_) => "Bybit",
			#[cfg(feature = "mexc")]
			Self::Mexc(_) => "Mexc",
		}
	}
}
impl std::fmt::Display for AbsMarket {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			#[cfg(feature = "binance")]
			Self::Binance(m) => write!(f, "{}/{}", self.exchange_name(), m),
			#[cfg(feature = "bybit")]
			Self::Bybit(m) => write!(f, "{}/{}", self.exchange_name(), m),
			#[cfg(feature = "mexc")]
			Self::Mexc(m) => write!(f, "{}/{}", self.exchange_name(), m),
		}
	}
}

impl std::str::FromStr for AbsMarket {
	type Err = eyre::Error;

	fn from_str(s: &str) -> Result<Self> {
		let parts: Vec<&str> = s.split('/').collect();
		if parts.len() != 2 {
			bail!("Invalid market string: {}", s);
		}
		let exchange = parts[0];
		let sub_market = parts[1];
		match exchange {
			#[cfg(feature = "binance")]
			"Binance" => Ok(Self::Binance(sub_market.parse()?)),

			#[cfg(feature = "bybit")]
			"Bybit" => Ok(Self::Bybit({
				match sub_market.parse() {
					Ok(m) => m,
					Err(e) => match sub_market.to_lowercase() == "futures" {
						true => crate::bybit::Market::Linear,
						false => bail!(e),
					},
				}
			})),
			#[cfg(feature = "mexc")]
			"Mexc" => Ok(Self::Mexc(sub_market.parse()?)),
			_ => bail!("Invalid market string: {}", s),
		}
	}
}
impl From<AbsMarket> for String {
	fn from(value: AbsMarket) -> Self {
		value.to_string()
	}
}
impl From<String> for AbsMarket {
	fn from(value: String) -> Self {
		value.parse().unwrap()
	}
}
impl From<&str> for AbsMarket {
	fn from(value: &str) -> Self {
		value.parse().unwrap()
	}
}
//,}}}

// Klines {{{
#[derive(Clone, Debug, Default, Copy)]
pub struct Oi {
	pub lsr: f64,
	pub total: f64,
	pub timestamp: DateTime<Utc>,
}

//Q: maybe add a `vectorize` method? Should add, question is really if it should be returning a) df b) all fields, including optional and oi c) t, o, h, l, c, v
// probably should figure out rust-typed dataframes for this first
#[derive(Clone, Debug, Default, Deref, DerefMut, derive_new::new)]
pub struct Klines {
	#[deref_mut]
	#[deref]
	pub v: VecDeque<Kline>,
	pub tf: Timeframe,
	/// Doesn't have to be synchronized with klines; each track has its own timestamps.
	pub oi: Vec<Oi>,
}
impl Iterator for Klines {
	type Item = Kline;

	fn next(&mut self) -> Option<Self::Item> {
		self.v.pop_front()
	}
}

//MOVE: v_utils (along with [Klines])
//? not sure what to do about oi here
/// [Kline]s series that is _guaranteed to not have any gaps in kline data_.
#[derive(Clone, Debug, Default)]
pub struct FullKlines(Klines);
impl TryFrom<Klines> for FullKlines {
	type Error = Report;

	fn try_from(value: Klines) -> Result<Self> {
		todo!();
	}
}
//,}}}

// RequestRange {{{
#[derive(Clone, Debug, Copy)]
pub enum RequestRange {
	/// Preferred way of defining the range
	StartEnd { start: DateTime<Utc>, end: Option<DateTime<Utc>> },
	/// For quick and dirty
	//TODO!: have it contain an enum, with either exact value, either just `Max`, then each exchange matches on it
	Limit(u32),
}
impl RequestRange {
	pub fn ensure_allowed(&self, allowed: std::ops::RangeInclusive<u32>, tf: &Timeframe) -> Result<(), RequestRangeError> {
		match self {
			RequestRange::StartEnd { start, end } =>
				if let Some(end) = end {
					if start > end {
						return Err(eyre!("Start time is greater than end time").into());
					}
					let effective_limit = ((*end - start).num_milliseconds() / tf.duration().num_milliseconds()) as u32;
					if effective_limit > *allowed.end() {
						return Err(OutOfRangeError::new(allowed, effective_limit).into());
					}
				},
			RequestRange::Limit(limit) =>
				if !allowed.contains(limit) {
					return Err(OutOfRangeError::new(allowed, *limit).into());
				},
		}
		Ok(())
	}

	//XXX
	//TODO!!!!!!!!!: MUST be generic over Market. But with current Market representation is impossible.
	pub fn serialize(&self, am: AbsMarket) -> serde_json::Value {
		match am {
			#[cfg(feature = "binance")]
			AbsMarket::Binance(_) => self.serialize_common(),
			#[cfg(feature = "bybit")]
			AbsMarket::Bybit(_) => self.serialize_common(),
			_ => unimplemented!(),
		}
	}

	fn serialize_common(&self) -> serde_json::Value {
		filter_nulls(match self {
			RequestRange::StartEnd { start, end } => json!({
				"startTime": start.timestamp_millis(),
				"endTime": end.map(|dt| dt.timestamp_millis()),
			}),
			RequestRange::Limit(limit) => json!({
				"limit": limit,
			}),
		})
	}
}
impl Default for RequestRange {
	fn default() -> Self {
		RequestRange::StartEnd {
			start: DateTime::default(),
			end: None,
		}
	}
}
impl From<DateTime<Utc>> for RequestRange {
	fn from(value: DateTime<Utc>) -> Self {
		RequestRange::StartEnd { start: value, end: None }
	}
}
/// funky
impl From<TimeDelta> for RequestRange {
	fn from(value: TimeDelta) -> Self {
		let now = Utc::now();
		RequestRange::StartEnd { start: now - value, end: None }
	}
}
impl From<u32> for RequestRange {
	fn from(value: u32) -> Self {
		RequestRange::Limit(value)
	}
}
impl From<i32> for RequestRange {
	fn from(value: i32) -> Self {
		RequestRange::Limit(value as u32)
	}
}
impl From<u16> for RequestRange {
	fn from(value: u16) -> Self {
		RequestRange::Limit(value as u32)
	}
}
impl From<u8> for RequestRange {
	fn from(value: u8) -> Self {
		RequestRange::Limit(value as u32)
	}
}
impl From<(DateTime<Utc>, DateTime<Utc>)> for RequestRange {
	fn from(value: (DateTime<Utc>, DateTime<Utc>)) -> Self {
		RequestRange::StartEnd { start: value.0, end: Some(value.1) }
	}
}
impl From<(i64, i64)> for RequestRange {
	fn from(value: (i64, i64)) -> Self {
		RequestRange::StartEnd {
			start: DateTime::from_timestamp_millis(value.0).unwrap(),
			end: Some(DateTime::from_timestamp_millis(value.1).unwrap()),
		}
	}
}

#[derive(Error, Debug, derive_more::Display, derive_more::From)]
pub enum RequestRangeError {
	OutOfRange(OutOfRangeError),
	Others(Report),
}
#[derive(derive_more::Debug, derive_new::new, thiserror::Error)]
pub struct OutOfRangeError {
	allowed: std::ops::RangeInclusive<u32>,
	provided: u32,
}
impl std::fmt::Display for OutOfRangeError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"Effective provided limit is out of range (could be translated from Start:End / tf). Allowed: {:?}, provided: {}",
			self.allowed, self.provided
		)
	}
}
//,}}}

// Balance {{{
#[derive(Clone, Debug, Default, Copy, derive_more::Deref, derive_more::DerefMut)]
pub struct AssetBalance {
	pub asset: Asset,
	pub underlying: f64,
	/// Optional, as for most exchanges appending it costs another call to `price{s}` endpoint
	#[deref_mut]
	#[deref]
	pub usd: Option<Usd>,
	// Binance
	//cross_wallet_balance: f64,
	//cross_unrealized_pnl: f64,
	//available_balance: f64,
	//max_withdraw_amount: f64,
	//margin_available: bool,
	// Mexc
	//available_balance: f64,
	//available_cash: f64,
	//available_open: f64,
	//bonus: f64,
	//cash_balance: f64,
	//currency: String,
	//equity: f64,
	//frozen_balance: f64,
	//position_margin: f64,
	//unrealized: f64,
}
#[derive(Clone, Debug, Default, derive_new::new, derive_more::Deref, derive_more::DerefMut)]
pub struct Balances {
	#[deref_mut]
	#[deref]
	v: Vec<AssetBalance>,
	/// breaks zero-cost of the abstraction, but I assume that most calls to this actually want usd, so it's warranted.
	pub total: Usd,
}
//,}}}

// Exchange Info {{{
#[derive(Clone, Debug, Default)]
pub struct ExchangeInfo {
	pub server_time: DateTime<Utc>,
	pub pairs: BTreeMap<Pair, PairInfo>,
}
impl ExchangeInfo {
	pub fn usdt_pairs(&self) -> impl Iterator<Item = Pair> {
		self.pairs.keys().filter(|p| p.is_usdt()).copied()
	}
}
#[derive(Clone, Debug, Default)]
pub struct PairInfo {
	pub price_precision: u8,
}
//,}}}
