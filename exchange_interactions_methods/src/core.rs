use std::collections::{BTreeMap, VecDeque};

use adapters::{
	Client, HttpClient,
	generics::{RetryConfig, ws::WsError},
};
use derive_more::{Deref, DerefMut};
use jiff::Timestamp;
use secrecy::SecretString;
use serde_json::json;
pub use trading_data_core::{BookShape, BookUpdate, ExchangeName, Instrument, PrecisionPriceQty, Symbol};
use v_utils::utils::filter_nulls;

use crate::{
	error::{ExchangeError, ExchangeResult, MethodError, OutOfRangeError, RequestRangeError},
	prelude::*,
};

const MAX_RECV_WINDOW: std::time::Duration = std::time::Duration::from_secs(10 * 60); // 10 minutes

pub use trading_data_core::{BatchTrades, InnerTrade};

/// Feature-gated exchange construction. Local extension trait over the foreign [`ExchangeName`]
/// (defined in `trading_data_core`) — the only piece of exchange behavior that cannot leave this crate.
pub trait ExchangeInit {
	fn init_client(&self) -> Box<dyn Exchange>;
	fn init_mock_client(&self) -> Box<dyn Exchange>;
}

/// Per-exchange book-event ordering token. Used internally by the WS connection to detect gaps
/// in the per-pair delta chain. Not persisted; the *result* (`gapped: bool`) is.
pub trait Sequence: Send + Sync {
	fn has_gap_from_prev(&self, prev: &Self) -> bool;
}

#[async_trait::async_trait]
pub trait SubscribeOrder {
	type Order;

	async fn place_and_subscribe(&mut self, topics: Vec<Self::Order>) -> Result<(), WsError>;
}

/// Concerns itself with exact types.
#[async_trait::async_trait]
pub trait ExchangeStream: std::fmt::Debug + Send + Sync {
	type Item;

	async fn next(&mut self) -> eyre::Result<Vec<Self::Item>, WsError>;
}

/// Reference data and the venue's request API. Every venue has one, so it is the base every other
/// capability is stated against.
#[async_trait::async_trait]
pub trait Market: std::fmt::Debug + Send + Sync + std::ops::Deref<Target = Client> + std::ops::DerefMut {
	fn name(&self) -> ExchangeName;

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
}

/// Everything behind an api key.
///
/// Each **private** method allows to specify `recv_window`; pass it through
/// [`validate_recv_window`] before the venue sees it.
#[async_trait::async_trait]
pub trait Account: Market {
	fn auth(&mut self, pubkey: String, secret: SecretString);
	/// Set number of **milliseconds** the request is valid for. Recv Window of over a minute does not make sense, thus it's expressed as u16.
	///
	/// **WARNING:** This sets a global default and should only be used as a crutch when you can't pass `recv_window` per-request.
	/// Prefer using the `recv_window` parameter in individual method calls instead.
	fn set_recv_window(&mut self, recv_window: std::time::Duration);
	/// Get the default recv_window configured for this exchange, if any.
	fn default_recv_window(&self) -> Option<std::time::Duration>;

	#[allow(unused_variables)]
	async fn personal_info(&self, instrument: Instrument, recv_window: Option<std::time::Duration>) -> ExchangeResult<PersonalInfo> {
		Err(ExchangeError::Method(MethodError::new_method_not_supported(self.name(), instrument)))
	}
}

//? potentially `total_balance`? Would return precompiled USDT-denominated balance of a (bybit::wallet/binance::account)
// balances are defined for each margin type: [futures_balance, spot_balance, margin_balance], but note that on some exchanges, (like bybit), some of these may point to the same exact call
// to negate confusion could add a `total_balance` endpoint

//? could implement many things that are _explicitly_ combinatorial. I can imagine several cases, where knowing that say the specified limit for the klines is wayyy over the max and that you may be opting into a long wait by calling it, could be useful.

/// Live event feeds. Not every venue has one — a venue that doesn't simply doesn't implement this,
/// and [`Exchange::stream`] answers `None` rather than a runtime "not supported".
#[async_trait::async_trait]
pub trait Stream: Market {
	async fn ws_trades(&mut self, pairs: &[Pair], instrument: Instrument) -> ExchangeResult<Box<dyn ExchangeStream<Item = BatchTrades>>>;
	/// Orderbook depth updates (max depth only).
	async fn ws_book(&mut self, pairs: &[Pair], instrument: Instrument) -> ExchangeResult<Box<dyn ExchangeStream<Item = BookUpdate>>>;
}

/// Bulk historical data, read from the venue's archive rather than its request API.
///
/// The return types are [`Stream`]'s exactly: a replay of the archive and a live socket produce the
/// same events, so a consumer takes both on one path. It also bounds memory — the stream pulls one
/// archive unit at a time rather than materialising the window.
///
/// Raw archive bytes land in `/tmp` and never cross this boundary; what comes back is already
/// scaled to the venue's own tick, which the implementor resolves through its own
/// [`Market::exchange_info`].
#[async_trait::async_trait]
pub trait History: Market {
	async fn trades(&self, symbol: Symbol, since: Timestamp, until: Timestamp) -> ExchangeResult<Box<dyn ExchangeStream<Item = BatchTrades>>>;
	async fn book(&self, symbol: Symbol, since: Timestamp, until: Timestamp) -> ExchangeResult<Box<dyn ExchangeStream<Item = BookUpdate>>>;
}

/// A venue, with whatever it happens to be able to do.
///
/// //NB: NEVER implement this trait manually. It is auto-implemented via blanket impl for all `ExchangeSeal` implementors.
/// The blanket impl ensures that this trait can only be implemented within this crate.
pub trait Exchange: Market + Account {
	fn set_timeout(&mut self, timeout: std::time::Duration);
	fn set_retry_config(&mut self, config: RetryConfig);
	fn set_use_testnet(&mut self, b: bool);
	fn set_cache_testnet_calls(&mut self, duration: Option<std::time::Duration>);
	fn stream(&mut self) -> Option<&mut dyn Stream>;
	fn history(&self) -> Option<&dyn History>;
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

#[derive(Clone, Debug, Eq, PartialEq, strum::Display)]
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
	pub price_precision: Precision,
	pub qty_precision: Precision,
	/// `None` means perpetual (no expiry). Only set for dated futures.
	pub delivery_date: Option<Timestamp>,
}
impl ExchangeInit for ExchangeName {
	fn init_client(&self) -> Box<dyn Exchange> {
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

	fn init_mock_client(&self) -> Box<dyn Exchange> {
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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Ticker {
	pub symbol: Symbol,
	pub exchange_name: ExchangeName,
}
// `InnerTrade`/`BatchTrades` moved to `trading_data_core` — the shared parse boundary so persistence
// can convert a ws batch straight into rows. Re-exported here so this crate's paths are unchanged.

/// The seal, and where a venue states which of the acquired capabilities it has: `Exchange` can
/// only be reached through this, and this is `pub(crate)`.
pub(crate) trait ExchangeSeal: Market + Account {
	fn stream(&mut self) -> Option<&mut dyn Stream> {
		None
	}

	fn history(&self) -> Option<&dyn History> {
		None
	}
}

/// Validates recv_window parameters and warns if using global default.
/// Returns an error if either the provided or default recv_window exceeds MAX_RECV_WINDOW.
pub(crate) fn validate_recv_window(recv_window: Option<std::time::Duration>, default_recv_window: Option<std::time::Duration>) -> ExchangeResult<()> {
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

impl<T: ExchangeSeal> Exchange for T {
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

	fn stream(&mut self) -> Option<&mut dyn Stream> {
		ExchangeSeal::stream(self)
	}

	fn history(&self) -> Option<&dyn History> {
		ExchangeSeal::history(self)
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
