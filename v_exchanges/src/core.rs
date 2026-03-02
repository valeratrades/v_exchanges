use std::collections::{BTreeMap, VecDeque};

use adapters::{Client, HttpClient, generics::ws::WsError};
use derive_more::{Deref, DerefMut};
use jiff::Timestamp;
use secrecy::SecretString;
use serde_json::json;
use v_utils::{
	prelude::*,
	trades::{Asset, Kline, Pair, Timeframe, Usd},
	utils::filter_nulls,
};

use crate::error::{ExchangeError, ExchangeResult, MethodError, OutOfRangeError, RequestRangeError};

const MAX_RECV_WINDOW: std::time::Duration = std::time::Duration::from_secs(10 * 60); // 10 minutes

/// Main trait for all standardized exchange interactions.
///
/// //NB: NEVER implement this trait manually. It is auto-implemented via blanket impl for all `ExchangeImpl` implementors.
/// The blanket impl ensures that this trait can only be implemented within this crate.
///
/// All HTTP methods (except websocket) are rate-limited by a semaphore that limits the number of
/// simultaneous outgoing requests. Use `set_max_simultaneous_requests` to configure the limit.
#[async_trait::async_trait]
pub trait Exchange: std::fmt::Debug + Send + Sync + std::ops::Deref<Target = Client> + std::ops::DerefMut {
	fn name(&self) -> ExchangeName;
	fn auth(&mut self, pubkey: String, secret: SecretString);
	fn set_recv_window(&mut self, recv_window: std::time::Duration);
	fn set_timeout(&mut self, timeout: std::time::Duration);
	fn set_retry_cooldown(&mut self, cooldown: std::time::Duration);
	fn set_max_tries(&mut self, max: u8);
	fn set_use_testnet(&mut self, b: bool);
	fn set_cache_testnet_calls(&mut self, duration: Option<std::time::Duration>);
	/// Set the maximum number of simultaneous requests allowed.
	/// Default is 100. The semaphore is shared across all clones of this exchange instance.
	fn set_max_simultaneous_requests(&mut self, max: usize);
	async fn exchange_info(&self, instrument: Instrument) -> ExchangeResult<ExchangeInfo>;
	async fn klines(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Klines>;
	async fn prices(&self, pairs: Option<Vec<Pair>>, instrument: Instrument) -> ExchangeResult<BTreeMap<Pair, f64>>;
	async fn price(&self, symbol: Symbol) -> ExchangeResult<f64>;
	async fn open_interest(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Vec<OpenInterest>>;
	async fn asset_balance(&self, asset: Asset, instrument: Instrument, recv_window: Option<std::time::Duration>) -> ExchangeResult<AssetBalance>;
	async fn balances(&self, instrument: Instrument, recv_window: Option<std::time::Duration>) -> ExchangeResult<Balances>;
	fn ws_trades(&self, pairs: Vec<Pair>, instrument: Instrument) -> ExchangeResult<Box<dyn ExchangeStream<Item = Trade>>>;
}
/// Concerns itself with exact types.
#[async_trait::async_trait]
pub trait ExchangeStream: std::fmt::Debug + Send + Sync {
	type Item;

	async fn next(&mut self) -> eyre::Result<Self::Item, WsError>;
}
#[async_trait::async_trait]
pub trait SubscribeOrder {
	type Order;

	async fn place_and_subscribe(&mut self, topics: Vec<Self::Order>) -> Result<(), WsError>;
}
/// most exchanges default to returning OI value in asset quantity, not quote. Exception would be Inverse on Bybit.
/// Which actually makes sense, as same endpoints accept things like "BTCETH", where quote value would be irrelevant.
#[derive(Clone, Copy, Debug, Default)]
pub struct OpenInterest {
	pub val_asset: f64,
	pub val_quote: Option<f64>,
	/// Binance's /futures/data/openInterestHist returns CMC's MC as well
	pub marketcap: Option<f64>,
	pub timestamp: Timestamp,
}
/// Does not have any gaps in the data, (as klines are meant to be indexed naively when used). TODO: enforce this.
///
/// # Arch
/// the greater the index, the newer the value
#[derive(Clone, Debug, Default, Deref, DerefMut, derive_new::new)]
pub struct Klines {
	#[deref_mut]
	#[deref]
	pub v: VecDeque<Kline>,
	pub tf: Timeframe,
}
#[derive(Clone, Copy, Debug)]
pub enum RequestRange {
	/// Preferred way of defining the range
	Span { since: Timestamp, until: Option<Timestamp> },
	/// For quick and dirty
	//TODO!: have it contain an enum, with either exact value, either just `Max`, then each exchange matches on it
	Limit(u32),
}
impl RequestRange {
	pub fn ensure_allowed(&self, allowed: std::ops::RangeInclusive<u32>, tf: &Timeframe) -> Result<(), RequestRangeError> {
		match self {
			RequestRange::Span { since: start, until: end } =>
				if let Some(end) = end {
					if start > end {
						return Err(eyre!("Start time is greater than end time").into());
					}
					let effective_limit =
						((*end - *start).get_milliseconds() / tf.duration().as_millis() as i64/*ok to downcast, because i64 will be sufficient for entirety of my lifetime*/) as u32;
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
			RequestRange::Span { since: start, until: end } => json!({
				"startTime": start.as_millisecond(),
				"endTime": end.map(|dt| dt.as_millisecond()),
			}),
			RequestRange::Limit(limit) => json!({
				"limit": limit,
			}),
		})
	}
}

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
#[derive(Clone, Debug, Default)]
pub struct ExchangeInfo {
	pub server_time: Timestamp,
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
#[derive(Clone, Debug, strum::Display, strum::EnumString, Eq, PartialEq)]
#[strum(serialize_all = "lowercase")]
#[non_exhaustive]
pub enum ExchangeName {
	Binance,
	Bybit,
	Kucoin,
	Mexc,
	BitFlyer,
	Coincheck,
	Yahoo,
}
impl ExchangeName {
	pub fn init_client(&self) -> Box<dyn Exchange> {
		match self {
			#[cfg(feature = "binance")]
			Self::Binance => Box::new(crate::Binance(Client::default())),
			#[cfg(feature = "bybit")]
			Self::Bybit => Box::new(crate::Bybit(Client::default())),
			#[cfg(feature = "kucoin")]
			Self::Kucoin => Box::new(crate::Kucoin(Client::default())),
			#[cfg(feature = "mexc")]
			Self::Mexc => Box::new(crate::Mexc(Client::default())),
			_ => unimplemented!(),
		}
	}

	pub fn init_mock_client(&self) -> Box<dyn Exchange> {
		match self {
			#[cfg(feature = "binance")]
			Self::Binance => Box::new(crate::Binance(Client::new_mock())),
			#[cfg(feature = "bybit")]
			Self::Bybit => Box::new(crate::Bybit(Client::new_mock())),
			#[cfg(feature = "kucoin")]
			Self::Kucoin => Box::new(crate::Kucoin(Client::new_mock())),
			#[cfg(feature = "mexc")]
			Self::Mexc => Box::new(crate::Mexc(Client::new_mock())),
			_ => unimplemented!(),
		}
	}
}

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, strum::Display, strum::EnumString, Eq, Hash, PartialEq, serde::Serialize)]
#[non_exhaustive]
pub enum Instrument {
	#[default]
	#[strum(serialize = "")]
	Spot,
	#[strum(serialize = ".P")]
	Perp,
	#[strum(serialize = ".M")]
	Margin, //Q: do we care for being able to parse spot/margin diff from ticker defs?
	#[strum(serialize = ".PERP_INVERSE")]
	PerpInverse,
	#[strum(serialize = ".OPTIONS")]
	Options,
}
#[derive(Clone, Debug)]
pub struct Ticker {
	pub symbol: Symbol,
	pub exchange_name: ExchangeName,
}
#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, Hash, PartialEq, serde::Serialize, derive_new::new)]
pub struct Symbol {
	pub pair: Pair,
	pub instrument: Instrument,
}
#[derive(Clone, Debug, Default)]
pub struct Trade {
	pub time: Timestamp,
	pub qty_asset: f64,
	pub price: f64,
}
pub struct BookSnapshot {
	pub time: Timestamp,
	pub asks: Vec<(f64, f64)>,
	pub bids: Vec<(f64, f64)>,
}
pub struct BookDelta {
	pub time: Timestamp,
	pub asks: Vec<(f64, f64)>,
	pub bids: Vec<(f64, f64)>,
}
/// Internal trait for exchange implementations.
/// Exchange implementations should implement this trait, not `Exchange` directly.
///
/// Each **private** method allows to specify `recv_window`.
///
/// # Other
/// - has too many methods, so for dev purposes most default to `unimplemented!()`.
#[async_trait::async_trait]
pub(crate) trait ExchangeImpl: std::fmt::Debug + Send + Sync + std::ops::Deref<Target = Client> + std::ops::DerefMut {
	fn name(&self) -> ExchangeName;
	// Config {{{
	fn auth(&mut self, pubkey: String, secret: SecretString);
	/// Set number of **milliseconds** the request is valid for. Recv Window of over a minute does not make sense, thus it's expressed as u16.
	///
	/// **WARNING:** This sets a global default and should only be used as a crutch when you can't pass `recv_window` per-request.
	/// Prefer using the `recv_window` parameter in individual method calls instead.
	fn set_recv_window(&mut self, recv_window: std::time::Duration);
	/// Get the default recv_window configured for this exchange, if any.
	fn default_recv_window(&self) -> Option<std::time::Duration>;
	//,}}}

	//Q: do we actually want to return a `MethodNotSupported` error, or should we just `unimplemented!()`?

	#[allow(unused_variables)]
	async fn exchange_info(&self, instrument: Instrument) -> ExchangeResult<ExchangeInfo> {
		Err(ExchangeError::Method(MethodError::MethodNotSupported { exchange: self.name(), instrument }))
	}

	//? should I have Self::Pair too? Like to catch the non-existent ones immediately? Although this would increase the error surface on new listings.
	#[allow(unused_variables)]
	async fn klines(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Klines> {
		Err(ExchangeError::Method(MethodError::MethodNotSupported {
			exchange: self.name(),
			instrument: symbol.instrument,
		}))
	}

	/// If no pairs are specified, returns for all;
	#[allow(unused_variables)]
	async fn prices(&self, pairs: Option<Vec<Pair>>, instrument: Instrument) -> ExchangeResult<BTreeMap<Pair, f64>> {
		Err(ExchangeError::Method(MethodError::MethodNotSupported { exchange: self.name(), instrument }))
	}

	#[allow(unused_variables)]
	async fn price(&self, symbol: Symbol) -> ExchangeResult<f64> {
		Err(ExchangeError::Method(MethodError::MethodNotSupported {
			exchange: self.name(),
			instrument: symbol.instrument,
		}))
	}

	/// Get Open Interest data
	/// in output vec: greater the index, fresher the data
	#[allow(unused_variables)]
	async fn open_interest(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Vec<OpenInterest>> {
		Err(ExchangeError::Method(MethodError::MethodNotSupported {
			exchange: self.name(),
			instrument: symbol.instrument,
		}))
	}

	// Authenticated {{{
	/// balance of a specific asset. Does not guarantee provision of USD values.
	#[allow(unused_variables)]
	async fn asset_balance(&self, asset: Asset, instrument: Instrument, recv_window: Option<std::time::Duration>) -> ExchangeResult<AssetBalance> {
		Err(ExchangeError::Method(MethodError::MethodNotSupported { exchange: self.name(), instrument }))
	}

	/// vec of _non-zero_ balances exclusively. Provides USD values.
	#[allow(unused_variables)]
	async fn balances(&self, instrument: Instrument, recv_window: Option<std::time::Duration>) -> ExchangeResult<Balances> {
		Err(ExchangeError::Method(MethodError::MethodNotSupported { exchange: self.name(), instrument }))
	}
	//,}}}

	//? potentially `total_balance`? Would return precompiled USDT-denominated balance of a (bybit::wallet/binance::account)
	// balances are defined for each margin type: [futures_balance, spot_balance, margin_balance], but note that on some exchanges, (like bybit), some of these may point to the same exact call
	// to negate confusion could add a `total_balance` endpoint

	//? could implement many things that are _explicitly_ combinatorial. I can imagine several cases, where knowing that say the specified limit for the klines is wayyy over the max and that you may be opting into a long wait by calling it, could be useful.

	// Websocket {{{
	// Start a websocket connection for individual trades
	#[allow(unused_variables)]
	fn ws_trades(&self, pairs: Vec<Pair>, instrument: Instrument) -> ExchangeResult<Box<dyn ExchangeStream<Item = Trade>>> {
		Err(ExchangeError::Method(MethodError::MethodNotSupported { exchange: self.name(), instrument }))
	}
	//,}}}
}
/// Validates recv_window parameters and warns if using global default.
/// Returns an error if either the provided or default recv_window exceeds MAX_RECV_WINDOW.
fn validate_recv_window(recv_window: Option<std::time::Duration>, default_recv_window: Option<std::time::Duration>) -> ExchangeResult<()> {
	if let Some(rw) = recv_window
		&& rw > MAX_RECV_WINDOW
	{
		return Err(ExchangeError::Other(eyre!("recv_window of {rw:?} exceeds maximum allowed duration of {MAX_RECV_WINDOW:?}")));
	}

	if let Some(rw) = default_recv_window
		&& rw > MAX_RECV_WINDOW
	{
		return Err(ExchangeError::Other(eyre!(
			"client's default recv_window of {rw:?} exceeds maximum allowed duration of {MAX_RECV_WINDOW:?}"
		)));
	}

	if recv_window.is_none() && default_recv_window.is_some() {
		tracing::warn!("called without recv_window, using global default (not recommended)");
	}

	Ok(())
}

/// Blanket impl: any type implementing ExchangeImpl automatically gets Exchange.
/// This enforces that Exchange can only be implemented within this crate (since ExchangeImpl is pub(crate)).
#[async_trait::async_trait]
impl<T: ExchangeImpl> Exchange for T {
	fn name(&self) -> ExchangeName {
		ExchangeImpl::name(self)
	}

	fn auth(&mut self, pubkey: String, secret: SecretString) {
		ExchangeImpl::auth(self, pubkey, secret)
	}

	fn set_recv_window(&mut self, recv_window: std::time::Duration) {
		ExchangeImpl::set_recv_window(self, recv_window)
	}

	fn set_timeout(&mut self, timeout: std::time::Duration) {
		self.http_client_mut().config.timeout = timeout;
	}

	fn set_retry_cooldown(&mut self, cooldown: std::time::Duration) {
		self.http_client_mut().config.retry_cooldown = cooldown;
	}

	fn set_max_tries(&mut self, max: u8) {
		self.http_client_mut().config.max_tries = max;
	}

	fn set_use_testnet(&mut self, b: bool) {
		self.http_client_mut().config.use_testnet = b;
	}

	fn set_cache_testnet_calls(&mut self, duration: Option<std::time::Duration>) {
		self.http_client_mut().config.cache_testnet_calls = duration;
	}

	fn set_max_simultaneous_requests(&mut self, max: usize) {
		(**self).set_max_simultaneous_requests(max);
	}

	async fn exchange_info(&self, instrument: Instrument) -> ExchangeResult<ExchangeInfo> {
		let _permit = self.request_semaphore().acquire().await.expect("semaphore closed");
		ExchangeImpl::exchange_info(self, instrument).await
	}

	async fn klines(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Klines> {
		let _permit = self.request_semaphore().acquire().await.expect("semaphore closed");
		ExchangeImpl::klines(self, symbol, tf, range).await
	}

	async fn prices(&self, pairs: Option<Vec<Pair>>, instrument: Instrument) -> ExchangeResult<BTreeMap<Pair, f64>> {
		let _permit = self.request_semaphore().acquire().await.expect("semaphore closed");
		ExchangeImpl::prices(self, pairs, instrument).await
	}

	async fn price(&self, symbol: Symbol) -> ExchangeResult<f64> {
		let _permit = self.request_semaphore().acquire().await.expect("semaphore closed");
		ExchangeImpl::price(self, symbol).await
	}

	async fn open_interest(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Vec<OpenInterest>> {
		let _permit = self.request_semaphore().acquire().await.expect("semaphore closed");
		ExchangeImpl::open_interest(self, symbol, tf, range).await
	}

	async fn asset_balance(&self, asset: Asset, instrument: Instrument, recv_window: Option<std::time::Duration>) -> ExchangeResult<AssetBalance> {
		validate_recv_window(recv_window, ExchangeImpl::default_recv_window(self))?;
		let _permit = self.request_semaphore().acquire().await.expect("semaphore closed");
		ExchangeImpl::asset_balance(self, asset, instrument, recv_window).await
	}

	async fn balances(&self, instrument: Instrument, recv_window: Option<std::time::Duration>) -> ExchangeResult<Balances> {
		validate_recv_window(recv_window, ExchangeImpl::default_recv_window(self))?;
		let _permit = self.request_semaphore().acquire().await.expect("semaphore closed");
		ExchangeImpl::balances(self, instrument, recv_window).await
	}

	// Websocket connections are NOT rate-limited by the semaphore
	fn ws_trades(&self, pairs: Vec<Pair>, instrument: Instrument) -> ExchangeResult<Box<dyn ExchangeStream<Item = Trade>>> {
		ExchangeImpl::ws_trades(self, pairs, instrument)
	}
}

// Open Interest {{{
//,}}}

// Klines {{{

//Q: maybe add a `vectorize` method? Should add, question is really if it should be returning a) df b) all fields, including optional and oi c) t, o, h, l, c, v
// probably should figure out rust-typed dataframes for this first
impl Iterator for Klines {
	type Item = Kline;

	fn next(&mut self) -> Option<Self::Item> {
		self.v.pop_front()
	}
}
//,}}}

// RequestRange {{{
impl Default for RequestRange {
	fn default() -> Self {
		RequestRange::Span {
			since: Timestamp::default(),
			until: None,
		}
	}
}
impl From<Timestamp> for RequestRange {
	fn from(value: Timestamp) -> Self {
		RequestRange::Span { since: value, until: None }
	}
}
impl From<jiff::Span> for RequestRange {
	fn from(time_delta: jiff::Span) -> Self {
		let now = Timestamp::now();
		RequestRange::Span {
			since: now - time_delta,
			until: None,
		}
	}
}
impl From<usize> for RequestRange {
	fn from(value: usize) -> Self {
		RequestRange::Limit(value as u32)
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
impl From<(Timestamp, Timestamp)> for RequestRange {
	fn from(value: (Timestamp, Timestamp)) -> Self {
		RequestRange::Span {
			since: value.0,
			until: Some(value.1),
		}
	}
}
impl From<(i64, i64)> for RequestRange {
	fn from(value: (i64, i64)) -> Self {
		RequestRange::Span {
			since: Timestamp::from_millisecond(value.0).unwrap(),
			until: Some(Timestamp::from_millisecond(value.1).unwrap()),
		}
	}
}
//,}}}

// Balance {{{
//,}}}

// Exchange Info {{{
//,}}}

// Ticker {{{

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

impl std::fmt::Display for Symbol {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}{}", self.pair, self.instrument)
	}
}

impl std::str::FromStr for Symbol {
	type Err = eyre::Report;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let (pair_str, instrument_ticker_str) = s.split_once('.').map(|(p, i)| (p, format!(".{}", i.to_uppercase()))).unwrap_or((s, "".to_owned()));
		let pair = Pair::from_str(pair_str)?;
		let instrument = Instrument::from_str(&instrument_ticker_str)?;

		Ok(Symbol { pair, instrument })
	}
}
impl From<&str> for Symbol {
	fn from(s: &str) -> Self {
		Self::from_str(s).unwrap()
	}
}
//,}}}

// Websocket {{{

//dbg: placeholder, ignore contents
//dbg: placeholder, ignore contents
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

	#[test]
	fn from_str_case_insensitive() {
		// Test lowercase instrument suffix
		let ticker_str = "binance:btc-usdt.p";
		let ticker: super::Ticker = ticker_str.parse().unwrap();
		assert_eq!(ticker.symbol.pair, super::Pair::new("BTC", "USDT"));
		assert_eq!(ticker.symbol.instrument, super::Instrument::Perp);
		assert_eq!(ticker.exchange_name, super::ExchangeName::Binance);

		// Test mixed case
		let ticker_str2 = "bybit:ETH-USDT.pErP_iNvErSe";
		let ticker2: super::Ticker = ticker_str2.parse().unwrap();
		assert_eq!(ticker2.symbol.instrument, super::Instrument::PerpInverse);
	}
}
