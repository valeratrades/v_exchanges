use std::collections::{BTreeMap, VecDeque};

use adapters::{
	Client, HttpClient,
	generics::{RetryConfig, ws::WsError},
};
use derive_more::{Deref, DerefMut};
use jiff::Timestamp;
use secrecy::SecretString;
use serde_json::json;
use v_exchanges_core::{Price, Timestamped};
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
#[async_trait::async_trait]
pub trait Exchange: std::fmt::Debug + Send + Sync + std::ops::Deref<Target = Client> + std::ops::DerefMut {
	fn name(&self) -> ExchangeName;
	fn auth(&mut self, pubkey: String, secret: SecretString);
	fn set_recv_window(&mut self, recv_window: std::time::Duration);
	fn set_timeout(&mut self, timeout: std::time::Duration);
	fn set_retry_config(&mut self, config: RetryConfig);
	fn set_use_testnet(&mut self, b: bool);
	fn set_cache_testnet_calls(&mut self, duration: Option<std::time::Duration>);
	async fn exchange_info(&mut self, instrument: Instrument) -> ExchangeResult<ExchangeInfo>;
	async fn klines(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Klines>;
	async fn prices(&self, pairs: Option<Vec<Pair>>, instrument: Instrument) -> ExchangeResult<BTreeMap<Pair, f64>>;
	async fn price(&self, symbol: Symbol) -> ExchangeResult<f64>;
	async fn open_interest(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Vec<OpenInterest>>;
	async fn personal_info(&self, instrument: Instrument, recv_window: Option<std::time::Duration>) -> ExchangeResult<PersonalInfo>;
	async fn ws_trades(&mut self, pairs: &[Pair], instrument: Instrument) -> ExchangeResult<Box<dyn ExchangeStream<Item = BatchTrades>>>;
	async fn ws_book(&mut self, pairs: &[Pair], instrument: Instrument) -> ExchangeResult<Box<dyn ExchangeStream<Item = BookUpdate>>>;
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
/// Pluggable sink that captures book updates as they fly through the WS connection. The connection
/// invokes `on_snapshot`/`on_delta` synchronously on every event, before returning from `next()`.
/// Implementations are expected to be cheap; heavy I/O should be batched internally.
pub trait BookPersistor: Send + Sync {
	fn on_snapshot(&mut self, pair: Pair, shape: &BookShape);
	fn on_delta(&mut self, pair: Pair, shape: &BookShape);
	/// Flush any in-memory buffers immediately. Called by callers at shutdown to avoid losing rows.
	fn flush(&mut self) {}
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
pub struct ApiKeyInfo {
	/// `None` means no expiry set (key is permanent)
	pub expire_time: Option<Timestamp>,
	/// Empty means the exchange doesn't expose permissions via this endpoint.
	pub permissions: Vec<KeyPermission>,
}

#[derive(Clone, Debug, strum::Display, Eq, PartialEq)]
#[non_exhaustive]
pub enum KeyPermission {
	/// Read-only access (market data, account info queries)
	Read,
	/// Spot trading
	SpotTrade,
	/// Futures/perpetual trading
	Futures,
	/// Options trading
	Options,
	/// Margin trading
	Margin,
	/// Withdrawals
	Withdraw,
	/// Asset transfers (internal, cross-account, sub-account)
	Transfer,
	/// Earn/savings products
	Earn,
	/// Anything not covered above
	Other(String),
}
impl KeyPermission {
	#[cfg(feature = "kucoin")]
	pub(crate) fn from_kucoin(s: &str) -> Self {
		match s {
			"General" => Self::Read,
			"Spot" => Self::SpotTrade,
			"Futures" => Self::Futures,
			"Options" => Self::Options,
			"Margin" => Self::Margin,
			"Withdrawal" => Self::Withdraw,
			"FlexTransfers" => Self::Transfer,
			"Earn" => Self::Earn,
			other => Self::Other(other.to_owned()),
		}
	}
}
#[derive(Clone, Debug)]
pub struct PersonalInfo {
	pub api: ApiKeyInfo,
	pub balances: Balances,
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
	pub qty_precision: u8,
	/// `None` means perpetual (no expiry). Only set for dated futures.
	pub delivery_date: Option<Timestamp>,
}
#[derive(Clone, Copy, Debug, strum::Display, strum::EnumString, Eq, Hash, PartialEq)]
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
			Self::Binance => Box::new(crate::Binance::default()),
			#[cfg(feature = "bybit")]
			Self::Bybit => Box::new(crate::Bybit::default()),
			#[cfg(feature = "kucoin")]
			Self::Kucoin => Box::new(crate::Kucoin::default()),
			#[cfg(feature = "mexc")]
			Self::Mexc => Box::new(crate::Mexc::default()),
			_ => unimplemented!(),
		}
	}

	pub fn init_mock_client(&self) -> Box<dyn Exchange> {
		match self {
			#[cfg(feature = "binance")]
			Self::Binance => Box::new(crate::Binance {
				client: Client::new_mock(),
				info_cache: BTreeMap::default(),
			}),
			#[cfg(feature = "bybit")]
			Self::Bybit => Box::new(crate::Bybit {
				client: Client::new_mock(),
				info_cache: BTreeMap::default(),
			}),
			#[cfg(feature = "kucoin")]
			Self::Kucoin => Box::new(crate::Kucoin {
				client: Client::new_mock(),
				info_cache: BTreeMap::default(),
			}),
			#[cfg(feature = "mexc")]
			Self::Mexc => Box::new(crate::Mexc {
				client: Client::new_mock(),
				info_cache: BTreeMap::default(),
			}),
			_ => unimplemented!(),
		}
	}
}

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, strum::Display, strum::EnumString, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize)]
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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Ticker {
	pub symbol: Symbol,
	pub exchange_name: ExchangeName,
}
#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, Hash, PartialEq, serde::Serialize, derive_new::new)]
pub struct Symbol {
	pub pair: Pair,
	pub instrument: Instrument,
}
/// Per-batch precision shared across all levels / trades in a [`BookShape`] / [`BatchTrades`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PrecisionPriceQty {
	pub price: u8,
	pub qty: u8,
}

impl PrecisionPriceQty {
	/// Strip the decimal point from a string and right-pad to `expected_precision` decimals.
	/// Trailing zeros beyond `expected_precision` are ignored (Binance pads `.24` to `.24000000`);
	/// any non-zero digit beyond `expected_precision` is a bug and panics.
	fn digits(s: &str, expected_precision: u8) -> String {
		match s.find('.') {
			Some(dot) => {
				let int_part = &s[..dot];
				let frac_part = &s[dot + 1..];
				let frac_significant = frac_part.trim_end_matches('0');
				let significant_decimals = frac_significant.len() as u8;
				assert!(
					significant_decimals <= expected_precision,
					"string {s:?} has {significant_decimals} significant decimal places, expected at most {expected_precision}"
				);
				let pad = expected_precision as usize - frac_significant.len();
				let mut out = String::with_capacity(int_part.len() + expected_precision as usize);
				out.push_str(int_part);
				out.push_str(frac_significant);
				for _ in 0..pad {
					out.push('0');
				}
				out
			}
			None => {
				let mut out = String::with_capacity(s.len() + expected_precision as usize);
				out.push_str(s);
				for _ in 0..expected_precision {
					out.push('0');
				}
				out
			}
		}
	}

	pub(crate) fn parse_price(&self, s: &str) -> i32 {
		Self::digits(s, self.price).parse().expect("price digits are valid i32")
	}

	pub(crate) fn parse_qty(&self, s: &str) -> u32 {
		Self::digits(s, self.qty).parse().expect("qty digits are valid u32")
	}
}

/// (price, qty) levels for both sides of an orderbook, keyed by raw price.
/// Both BTreeMaps are ascending; consumers reverse `bids` for best-bid.
#[derive(Clone, Debug, Default)]
pub struct BookShape {
	/// Exchange-provided event time.
	pub ts_event: Timestamp,
	/// When we first received the data backing this shape.
	pub ts_init: Timestamp,
	/// When we last wrote into this shape. Equals `ts_init` for shapes built from a single message.
	pub ts_last: Timestamp,
	pub prec: PrecisionPriceQty,
	pub asks: BTreeMap<i32, u32>,
	pub bids: BTreeMap<i32, u32>,
}

impl Timestamped for BookShape {
	fn ts_event(&self) -> Timestamp {
		self.ts_event
	}

	fn ts_init(&self) -> Timestamp {
		self.ts_init
	}

	fn ts_last(&self) -> Timestamp {
		self.ts_last
	}
}

/// Distinguishes full snapshots from incremental deltas.
/// For deltas: qty=0 means remove that price level.
#[derive(Clone, Debug)]
pub enum BookUpdate {
	Snapshot(BookShape),
	BatchDelta(BookShape),
}

impl BookUpdate {
	pub fn shape(&self) -> &BookShape {
		match self {
			Self::Snapshot(s) | Self::BatchDelta(s) => s,
		}
	}
}

impl Timestamped for BookUpdate {
	fn ts_event(&self) -> Timestamp {
		self.shape().ts_event
	}

	fn ts_init(&self) -> Timestamp {
		self.shape().ts_init
	}

	fn ts_last(&self) -> Timestamp {
		self.shape().ts_last
	}
}

/// Batched trade stream event. All trades share `prec`.
#[derive(Clone, Debug, Default)]
pub struct BatchTrades {
	prec: PrecisionPriceQty,
	trades: Vec<InnerTrade>,
	/// Exchange-provided event time of the latest trade in the batch.
	ts_event: Timestamp,
	/// When we first received the data backing this batch.
	ts_init: Timestamp,
	/// When we last appended into this batch. Equals `ts_init` for batches built from a single message.
	ts_last: Timestamp,
}

impl BatchTrades {
	pub(crate) fn new(prec: PrecisionPriceQty, trades: Vec<InnerTrade>, ts_init: Timestamp, ts_last: Timestamp) -> Self {
		assert!(trades.len() != 0); // this is an invariant upheld by our own implementation, so we shouldn't introduce runtime cost of checking it in release builds.
		let ts_event = trades.last().expect("never empty").time;
		Self {
			prec,
			trades,
			ts_event,
			ts_init,
			ts_last,
		}
	}

	pub fn len(&self) -> usize {
		self.trades.len()
	}

	pub fn last_price(&self) -> Price {
		Price::new(self.trades.last().expect("never empty").price, self.prec.price)
	}

	/// Iterate `(time, price_raw, qty_raw)` tuples. Precision is shared via [`Self::prec`].
	pub fn iter(&self) -> impl Iterator<Item = (Timestamp, i32, u32)> + '_ {
		self.trades.iter().map(|t| (t.time, t.price, t.qty))
	}
}

impl Timestamped for BatchTrades {
	fn ts_event(&self) -> Timestamp {
		self.ts_event
	}

	fn ts_init(&self) -> Timestamp {
		self.ts_init
	}

	fn ts_last(&self) -> Timestamp {
		self.ts_last
	}
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
	fn info_cache_mut(&mut self) -> &mut BTreeMap<Instrument, ExchangeInfo>;

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
		Err(ExchangeError::Method(MethodError::new_method_not_supported(self.name(), instrument)))
	}

	//? should I have Self::Pair too? Like to catch the non-existent ones immediately? Although this would increase the error surface on new listings.
	#[allow(unused_variables)]
	async fn klines(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Klines> {
		Err(ExchangeError::Method(MethodError::new_method_not_supported(self.name(), symbol.instrument)))
	}

	/// If no pairs are specified, returns for all;
	#[allow(unused_variables)]
	async fn prices(&self, pairs: Option<Vec<Pair>>, instrument: Instrument) -> ExchangeResult<BTreeMap<Pair, f64>> {
		Err(ExchangeError::Method(MethodError::new_method_not_supported(self.name(), instrument)))
	}

	#[allow(unused_variables)]
	/// NB: not perf-critical, so literally just calls `prices`, incurring cost of making a vec and a BTreeMap for no reason
	async fn price(&self, symbol: Symbol) -> ExchangeResult<f64> {
		self.prices(Some(vec![symbol.pair]), symbol.instrument).await.map(|m| m[&symbol.pair])
	}

	/// Get Open Interest data
	/// in output vec: greater the index, fresher the data
	#[allow(unused_variables)]
	async fn open_interest(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Vec<OpenInterest>> {
		Err(ExchangeError::Method(MethodError::new_method_not_supported(self.name(), symbol.instrument)))
	}

	// Authenticated {{{
	#[allow(unused_variables)]
	async fn personal_info(&self, instrument: Instrument, recv_window: Option<std::time::Duration>) -> ExchangeResult<PersonalInfo> {
		Err(ExchangeError::Method(MethodError::new_method_not_supported(self.name(), instrument)))
	}
	//,}}}

	//? potentially `total_balance`? Would return precompiled USDT-denominated balance of a (bybit::wallet/binance::account)
	// balances are defined for each margin type: [futures_balance, spot_balance, margin_balance], but note that on some exchanges, (like bybit), some of these may point to the same exact call
	// to negate confusion could add a `total_balance` endpoint

	//? could implement many things that are _explicitly_ combinatorial. I can imagine several cases, where knowing that say the specified limit for the klines is wayyy over the max and that you may be opting into a long wait by calling it, could be useful.

	// Websocket {{{
	// Start a websocket connection for individual trades
	#[allow(unused_variables)]
	async fn ws_trades(&mut self, pairs: &[Pair], instrument: Instrument) -> ExchangeResult<Box<dyn ExchangeStream<Item = BatchTrades>>> {
		Err(ExchangeError::Method(MethodError::new_method_not_supported(self.name(), instrument)))
	}

	/// Start a websocket connection for orderbook depth updates (max depth only).
	#[allow(unused_variables)]
	async fn ws_book(&mut self, pairs: &[Pair], instrument: Instrument) -> ExchangeResult<Box<dyn ExchangeStream<Item = BookUpdate>>> {
		Err(ExchangeError::Method(MethodError::new_method_not_supported(self.name(), instrument)))
	}
	//,}}}
}
#[derive(Clone, Debug, Default)]
pub(crate) struct InnerTrade {
	pub time: Timestamp,
	pub price: i32,
	pub qty: u32,
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

	fn set_retry_config(&mut self, config: RetryConfig) {
		self.http_client_mut().config.retry = config;
	}

	fn set_use_testnet(&mut self, b: bool) {
		self.http_client_mut().config.use_testnet = b;
	}

	fn set_cache_testnet_calls(&mut self, duration: Option<std::time::Duration>) {
		self.http_client_mut().config.cache_testnet_calls = duration;
	}

	async fn exchange_info(&mut self, instrument: Instrument) -> ExchangeResult<ExchangeInfo> {
		let info = ExchangeImpl::exchange_info(self, instrument).await?;
		self.info_cache_mut().insert(instrument, info.clone());
		Ok(info)
	}

	async fn klines(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Klines> {
		ExchangeImpl::klines(self, symbol, tf, range).await
	}

	async fn prices(&self, pairs: Option<Vec<Pair>>, instrument: Instrument) -> ExchangeResult<BTreeMap<Pair, f64>> {
		ExchangeImpl::prices(self, pairs, instrument).await
	}

	async fn price(&self, symbol: Symbol) -> ExchangeResult<f64> {
		ExchangeImpl::price(self, symbol).await
	}

	async fn open_interest(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Vec<OpenInterest>> {
		ExchangeImpl::open_interest(self, symbol, tf, range).await
	}

	async fn personal_info(&self, instrument: Instrument, recv_window: Option<std::time::Duration>) -> ExchangeResult<PersonalInfo> {
		validate_recv_window(recv_window, ExchangeImpl::default_recv_window(self))?;
		ExchangeImpl::personal_info(self, instrument, recv_window).await
	}

	// Websocket connections are NOT rate-limited by the semaphore
	async fn ws_trades(&mut self, pairs: &[Pair], instrument: Instrument) -> ExchangeResult<Box<dyn ExchangeStream<Item = BatchTrades>>> {
		ExchangeImpl::ws_trades(self, pairs, instrument).await
	}

	async fn ws_book(&mut self, pairs: &[Pair], instrument: Instrument) -> ExchangeResult<Box<dyn ExchangeStream<Item = BookUpdate>>> {
		ExchangeImpl::ws_book(self, pairs, instrument).await
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
