use std::collections::{BTreeMap, VecDeque};

use chrono::{DateTime, TimeDelta, Utc};
use derive_more::{Deref, DerefMut};
use eyre::{Report, Result, bail};
use serde_json::json;
use v_utils::{
	trades::{Asset, Kline, Pair, Timeframe},
	utils::filter_nulls,
};

//TODO!!!!!!!!!!!!!: klines switch to defining the range via an Enum over either limit either start and end times
#[async_trait::async_trait]
pub trait Exchange: std::fmt::Debug {
	fn source_market(&self) -> AbsMarket;
	fn exchange_name(&self) -> &'static str;
	fn auth(&mut self, key: String, secret: String);

	async fn exchange_info(&self, m: AbsMarket) -> Result<ExchangeInfo>;

	//? should I have Self::Pair too? Like to catch the non-existent ones immediately? Although this would increase the error surface on new listings.
	async fn klines(&self, pair: Pair, tf: Timeframe, range: RequestRange, m: AbsMarket) -> Result<Klines>;

	/// If no pairs are specified, returns for all;
	async fn prices(&self, pairs: Option<Vec<Pair>>, m: AbsMarket) -> Result<Vec<(Pair, f64)>>;
	async fn price(&self, pair: Pair, m: AbsMarket) -> Result<f64>;

	// Defined in terms of actors
	//TODO!!!: async fn spawn_klines_listener(&self, symbol: Pair, tf: Timeframe) -> mpsc::Receiver<Kline>;

	/// balance of a specific asset
	async fn asset_balance(&self, asset: Asset, m: AbsMarket) -> Result<AssetBalance>;
	/// vec of balances of specific assets
	async fn balances(&self, m: AbsMarket) -> Result<Vec<AssetBalance>>;
	//? potentially `total_balance`? Would return precompiled USDT-denominated balance of a (bybit::wallet/binance::account)
	// balances are defined for each margin type: [futures_balance, spot_balance, margin_balance], but note that on some exchanges, (like bybit), some of these may point to the same exact call
	// to negate confusion could add a `total_balance` endpoint

	//? could implement many things that are _explicitly_ combinatorial. I can imagine several cases, where knowing that say the specified limit for the klines is wayyy over the max and that you may be opting into a long wait by calling it, could be useful.
}

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
	fn client(&self) -> Box<dyn Exchange>;
	fn fmt_abs(&self) -> String;
	//TODO; require them to impl Display and FromStr
}

#[derive(Debug, Clone, Copy)]
pub enum AbsMarket {
	Binance(crate::binance::Market),
	Bybit(crate::bybit::Market),
	//TODO
}
impl AbsMarket {
	pub fn client(&self) -> Box<dyn Exchange> {
		match self {
			Self::Binance(m) => m.client(),
			Self::Bybit(m) => m.client(),
		}
	}

	pub fn exchange_name(&self) -> &'static str {
		match self {
			Self::Binance(_) => "Binance",
			Self::Bybit(_) => "Bybit",
		}
	}
}
impl Default for AbsMarket {
	fn default() -> Self {
		Self::Binance(crate::binance::Market::default())
	}
}
impl std::fmt::Display for AbsMarket {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Binance(m) => write!(f, "Binance/{}", m),
			Self::Bybit(m) => write!(f, "Bybit/{}", m),
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
		match exchange.to_lowercase().as_str() {
			"binance" => Ok(Self::Binance(sub_market.parse()?)),
			"bybit" => Ok(Self::Bybit({
				match sub_market.parse() {
					Ok(m) => m,
					Err(e) => match sub_market.to_lowercase() == "futures" {
						true => crate::bybit::Market::Linear,
						false => bail!(e),
					},
				}
			})),
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
#[derive(Clone, Debug, Default, Deref, DerefMut)]
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
	pub fn ensure_allowed(&self, allowed: std::ops::RangeInclusive<u32>, tf: Timeframe) -> Result<()> {
		match self {
			RequestRange::StartEnd { start, end } =>
				if let Some(end) = end {
					if start > end {
						bail!("Start time is greater than end time");
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
	pub fn serialize(&self) -> serde_json::Value {
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

#[derive(Clone, Debug, Default, Copy)]
pub struct AssetBalance {
	pub asset: Asset,
	pub balance: f64,
	//cross_wallet_balance: f64,
	//cross_unrealized_pnl: f64,
	//available_balance: f64,
	//max_withdraw_amount: f64,
	//margin_available: bool,
	pub timestamp: i64,
}

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
