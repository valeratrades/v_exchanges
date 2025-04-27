use std::collections::{BTreeMap, VecDeque};

use adapters::{
	generics::{http::RequestError, ws::{Topic, WsError}}, Client
};
use chrono::{DateTime, TimeDelta, Utc};
use derive_more::{Deref, DerefMut};
use crate::define_str_enum;
use secrecy::SecretString;
use serde_json::json;
use tokio::sync::mpsc;
use v_utils::{
	prelude::*,
	trades::{Asset, Kline, Pair, Timeframe, Usd},
	utils::filter_nulls,
};

/// Main trait for all standardized exchange interactions
///
/// Each **private** method allows to specify `recv_window`.
///
/// # Other
/// - has too many methods, so for dev purposes most default to `unimplemented!()`.
#[async_trait::async_trait]
pub trait Exchange: std::fmt::Debug + Send + Sync {
	fn name(&self) -> ExchangeName;
	#[doc(hidden)]
	/// Only need for the auto-impls of config setters
	fn __client_mut(&mut self) -> &mut Client;
	// Config {{{
	fn auth(&mut self, pubkey: String, secret: SecretString);
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
	fn set_use_testnes(&mut self, b: bool) {
		self.__client_mut().client.config.use_testnet = b;
	}
	fn set_cache_testnet_calls(&mut self, duration: Option<std::time::Duration>) {
		self.__client_mut().client.config.cache_testnet_calls = duration;
	}
	//DO: same for other fields in [RequestConfig](v_exchanges_api_generics::http::RequestConfig)
	//,}}}

	#[allow(unused_variables)]
	async fn exchange_info(&self, instrument: Instrument) -> ExchangeResult<ExchangeInfo> {
		unimplemented!();
	}

	//? should I have Self::Pair too? Like to catch the non-existent ones immediately? Although this would increase the error surface on new listings.
	#[allow(unused_variables)]
	async fn klines(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Klines> {
		unimplemented!();
	}

	/// If no pairs are specified, returns for all;
	#[allow(unused_variables)]
	async fn prices(&self, pairs: Option<Vec<Pair>>, instrument: Instrument) -> ExchangeResult<BTreeMap<Pair, f64>> {
		unimplemented!();
	}
	#[allow(unused_variables)]
	async fn price(&self, symbol: Symbol) -> ExchangeResult<f64> {
		unimplemented!();
	}

	/// balance of a specific asset. Does not guarantee provision of USD values.
	#[allow(unused_variables)]
	async fn asset_balance(&self, asset: Asset, recv_window: Option<u16>, instrument: Instrument) -> ExchangeResult<AssetBalance> {
		unimplemented!();
	}
	/// vec of _non-zero_ balances exclusively. Provides USD values.
	#[allow(unused_variables)]
	async fn balances(&self, recv_window: Option<u16>, instrument: Instrument) -> ExchangeResult<Balances> {
		unimplemented!();
	}

	//? potentially `total_balance`? Would return precompiled USDT-denominated balance of a (bybit::wallet/binance::account)
	// balances are defined for each margin type: [futures_balance, spot_balance, margin_balance], but note that on some exchanges, (like bybit), some of these may point to the same exact call
	// to negate confusion could add a `total_balance` endpoint

	//? could implement many things that are _explicitly_ combinatorial. I can imagine several cases, where knowing that say the specified limit for the klines is wayyy over the max and that you may be opting into a long wait by calling it, could be useful.

	// Start a websocket connection for individual trades
	#[allow(unused_variables)]
	async fn ws_trades(
		&self,
		symbol: Symbol,
	) -> ExchangeResult<
		mpsc::Receiver<
			Result<
				crate::core::TradeEvent,
				WsError, /*The key could get out of the date while the connection is ongoing, and then the next attempt to reconnect will fail. Thus must encapsulate received type in WsError.*/
			>,
		>,
	> {
		unimplemented!();
	}
}

// Exchange Error {{{
pub type ExchangeResult<T> = Result<T, ExchangeError>;
#[derive(Debug, derive_more::Display, Error, derive_more::From)]
pub enum ExchangeError {
	Request(RequestError),
	Method(MethodError),
	Timeframe(UnsupportedTimeframeError),
	Range(RequestRangeError),
	Other(Report),
}
#[derive(Debug, Error, derive_new::new)]
#[error("Chosen exchange does not support the requested timeframe. Provided: {provided}, allowed: {allowed:?}")]
pub struct UnsupportedTimeframeError {
	provided: Timeframe,
	allowed: Vec<Timeframe>,
}
#[derive(Debug, thiserror::Error, derive_new::new)]
pub enum MethodError {
	/// Means that it's **not expected** to be implemented, not only that it's not implemented now. For things that are yet to be implemented I just put `unimplemented!()`.
	#[error("Method not implemented for the requested exchange and instrument: ({exchange}, {instrument})")]
	MethodNotImplemented{exchange: ExchangeName, instrument: Instrument},
	#[error("Requested exchange does not support the method for chosen instrument: ({exchange}, {instrument})")]
	MethodNotSupported{exchange: ExchangeName, instrument: Instrument},
}
//,}}}

// Klines {{{
#[derive(Clone, Copy, Debug, Default)]
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
#[derive(Clone, Copy, Debug)]
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

	pub fn serialize(&self, exchange: ExchangeName) -> serde_json::Value {
		match exchange {
			#[cfg(feature = "binance")]
			ExchangeName::Binance => self.serialize_common(),
			#[cfg(feature = "bybit")]
			ExchangeName::Bybit => self.serialize_common(),
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

#[derive(Debug, derive_more::Display, Error, derive_more::From)]
pub enum RequestRangeError {
	OutOfRange(OutOfRangeError),
	Others(Report),
}
#[derive(derive_more::Debug, thiserror::Error, derive_new::new)]
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
#[derive(Clone, Copy, Debug, Default, derive_more::Deref, derive_more::DerefMut)]
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
#[derive(Clone, Debug, Default, derive_more::Deref, derive_more::DerefMut, derive_new::new)]
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

// Ticker {{{

define_str_enum! {
	#[derive(Clone, Debug, derive_more::From, PartialEq, Eq)]
	#[non_exhaustive]
	pub enum ExchangeName {
		Binance => "binance",
		Bybit => "bybit",
		Mexc => "mexc",
		BitFlyer => "bitflyer",
		Coincheck => "coincheck",
		Yahoo => "yahook",
	}
}
impl ExchangeName {
	pub fn build_client(&self) -> Box<dyn Exchange> {
		match self {
			#[cfg(feature = "binance")]
			Self::Binance => Box::new(crate::Binance(Client::default())),
			#[cfg(feature = "bybit")]
			Self::Bybit => Box::new(crate::Bybit(Client::default())),
			#[cfg(feature = "mexc")]
			Self::Mexc => Box::new(crate::Mexc(Client::default())),
			_ => unimplemented!(),
		}
	}
}

define_str_enum! {
	#[derive(Clone, Debug, derive_more::From, PartialEq, Eq)]
	#[non_exhaustive]
	pub enum Instrument {
		Spot => "",
		Perp => ".P",
		Margin => ".M", //Q: do we care for being able to parse spot/margin diff from ticker defs?
		PerpInverse => ".PERP_INVERSE",
		Options => ".OPTIONS",
	}
}

pub struct Ticker {
	pub symbol: Symbol,
	pub exchange_name: ExchangeName,
}

impl std::fmt::Display for Ticker {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}:{}", self.exchange_name, self.symbol)
	}
}

impl std::str::FromStr for Ticker {
	type Err = eyre::Report;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let (exchange_str, symbol_str) = s.split_once(':').ok_or_else(|| eyre::eyre!("Invalid ticker format"))?;
		let exchange_name = ExchangeName::from_str(exchange_str)?;
		let symbol = Symbol::from_str(symbol_str)?;

		Ok(Ticker { symbol, exchange_name })
	}
}

pub struct Symbol {
	pub pair: Pair,
	pub instrument: Instrument,
}

impl std::fmt::Display for Symbol {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}{}", self.pair, self.instrument)
	}
}

impl std::str::FromStr for Symbol {
	type Err = eyre::Report;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let (pair_str, instrument_ticker_str) = s.split_once('.').map(|(p, i)| (p, format!(".{i}"))).unwrap_or((s, "".to_owned()));
		let pair = Pair::from_str(pair_str)?;
		let instrument = Instrument::from_str(&instrument_ticker_str)?;

		Ok(Symbol { pair, instrument })
	}
}
//,}}}

// Websocket {{{
#[async_trait::async_trait]
pub trait ExchangeStream {
	type Content;
	type Topic;

	async fn next(&mut self) -> Option<Result<Self::Content, WsError>>;
	async fn subscribe(&mut self, topics: Vec<Topic>) -> Result<(), WsError>;
}

#[derive(Clone, Debug, Default)]
pub struct TradeEvent {
	pub time: DateTime<Utc>,
	pub qty_asset: f64,
	pub price: f64,
}

//dbg: placeholder, ignore contents
pub struct BookSnapshot {
	pub time: DateTime<Utc>,
	pub asks: Vec<(f64, f64)>,
	pub bids: Vec<(f64, f64)>,
}
//dbg: placeholder, ignore contents
pub struct BookDelta {
	pub time: DateTime<Utc>,
	pub asks: Vec<(f64, f64)>,
	pub bids: Vec<(f64, f64)>,
}
//,}}}

mod test {
	#[test]
	fn display() {
		let symbol = super::Symbol {
			pair: super::Pair::new("BTC", "USDT"),
			instrument: super::Instrument::Perp,
		};
		let ticker = super::Ticker {
			symbol,
			exchange_name: super::ExchangeName::Bybit,
		};
		assert_eq!(ticker.to_string(), "bybit:BTC-USDT.P");
	}

	#[test]
	fn from_str() {
		let ticker_str = "bybit:BTC-USDT.P";
		let ticker: super::Ticker = ticker_str.parse().unwrap();
		assert_eq!(ticker.symbol.pair, super::Pair::new("BTC", "USDT"));
		assert_eq!(ticker.symbol.instrument, super::Instrument::Perp);
		assert_eq!(ticker.exchange_name, super::ExchangeName::Bybit);
	}
}
