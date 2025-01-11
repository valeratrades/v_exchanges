use std::collections::{HashMap, VecDeque};

use chrono::{DateTime, TimeDelta, Utc};
use derive_more::{Deref, DerefMut};
use eyre::{Report, Result};
use v_utils::trades::{Asset, Kline, Pair, Timeframe};

//TODO!!!!!!!!!!!!!: klines switch to defining the range via an Enum over either limit either start and end times

pub trait Exchange {
	type M: MarketTrait;

	fn auth<S: Into<String>>(&mut self, key: S, secret: S);

	fn exchange_info(&self, m: Self::M) -> impl std::future::Future<Output = Result<ExchangeInfo>> + Send;

	//? should I have Self::Pair too? Like to catch the non-existent ones immediately? Although this would increase the error surface on new listings.
	fn klines(&self, pair: Pair, tf: Timeframe, range: KlinesRequestRange, m: Self::M) -> impl std::future::Future<Output = Result<Klines>> + Send;

	/// If no pairs are specified, returns for all;
	fn prices(&self, pairs: Option<Vec<Pair>>, m: Self::M) -> impl std::future::Future<Output = Result<Vec<(Pair, f64)>>> + Send;
	fn price(&self, pair: Pair, m: Self::M) -> impl std::future::Future<Output = Result<f64>> + Send;

	// Defined in terms of actors
	//TODO!!!: fn spawn_klines_listener(&self, symbol: Pair, tf: Timeframe) -> mpsc::Receiver<Kline>;

	/// balance of a specific asset
	fn asset_balance(&self, asset: Asset, m: Self::M) -> impl std::future::Future<Output = Result<AssetBalance>> + Send;
	/// vec of balances of specific assets
	fn balances(&self, m: Self::M) -> impl std::future::Future<Output = Result<Vec<AssetBalance>>> + Send;
	//? potentially `total_balance`? Would return precompiled USDT-denominated balance of a (bybit::wallet/binance::account)
	// balances are defined for each margin type: [futures_balance, spot_balance, margin_balance], but note that on some exchanges, (like bybit), some of these may point to the same exact call
	// to negate confusion could add a `total_balance` endpoint

	//? could implement many things that are _explicitly_ combinatorial. I can imagine several cases, where knowing that say the specified limit for the klines is wayyy over the max and that you may be opting into a long wait by calling it, could be useful.
}

// Market {{{
pub trait MarketTrait {
	type Client: Exchange;
	fn client(&self) -> Self::Client;
}
//TODO!: figure out how can I expose one central `Market` enum, so client doesn't have to bring into the scope `MarketTrait` and deal with the exchange-specific `Market`'s type
// Maybe [enum_dispatch](<https://docs.rs/enum_dispatch/latest/enum_dispatch/>) crate could help?

//#[derive(Debug, Clone, Copy)]
//pub enum Market {
//	Binance(crate::binance::Market),
//	Bybit(crate::bybit::Market),
//	//TODO
//}
//
//impl Default for Market {
//	fn default() -> Self {
//		Self::Binance(crate::binance::Market::default())
//	}
//}
//
//impl std::fmt::Display for Market {
//	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//		match self {
//			Market::Binance(m) => write!(f, "Binance/{}", m),
//			Market::Bybit(m) => write!(f, "Bybit/{}", m),
//		}
//	}
//}
//
//impl std::str::FromStr for Market {
//	type Err = eyre::Error;
//
//	fn from_str(s: &str) -> Result<Self> {
//		let parts: Vec<&str> = s.split('/').collect();
//		if parts.len() != 2 {
//			return Err(eyre::eyre!("Invalid market string: {}", s));
//		}
//		let exchange = parts[0];
//		let sub_market = parts[1];
//		match exchange.to_lowercase().as_str() {
//			"binance" => Ok(Self::Binance(sub_market.parse()?)),
//			"bybit" => Ok(Self::Bybit({
//				match sub_market.parse() {
//					Ok(m) => m,
//					Err(e) => match sub_market.to_lowercase() == "futures" {
//						true => crate::bybit::Market::Linear,
//						false => eyre::bail!(e),
//					}
//				}
//			})),
//			_ => bail!("Invalid market string: {}", s),
//		}
//	}
//}
//impl From<Market> for String {
//	fn from(value: Market) -> Self {
//		value.to_string()
//	}
//}
//impl From<String> for Market {
//	fn from(value: String) -> Self {
//		value.parse().unwrap()
//	}
//}
//impl From<&str> for Market {
//	fn from(value: &str) -> Self {
//		value.parse().unwrap()
//	}
//}
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

#[derive(Clone, Debug, Copy)]
pub enum KlinesRequestRange {
	/// Preferred way of defining the range
	StartEnd { start: DateTime<Utc>, end: Option<DateTime<Utc>> },
	/// For quick and dirty
	//TODO!: have it contain an enum, with either exact value, either just `Max`, then each exchange matches on it
	Limit(u32),
}
impl Default for KlinesRequestRange {
	fn default() -> Self {
		KlinesRequestRange::StartEnd {
			start: DateTime::default(),
			end: None,
		}
	}
}
impl From<DateTime<Utc>> for KlinesRequestRange {
	fn from(value: DateTime<Utc>) -> Self {
		KlinesRequestRange::StartEnd { start: value, end: None }
	}
}
/// funky
impl From<TimeDelta> for KlinesRequestRange {
	fn from(value: TimeDelta) -> Self {
		let now = Utc::now();
		KlinesRequestRange::StartEnd { start: now - value, end: None }
	}
}
impl From<u32> for KlinesRequestRange {
	fn from(value: u32) -> Self {
		KlinesRequestRange::Limit(value)
	}
}
impl From<i32> for KlinesRequestRange {
	fn from(value: i32) -> Self {
		KlinesRequestRange::Limit(value as u32)
	}
}
impl From<u16> for KlinesRequestRange {
	fn from(value: u16) -> Self {
		KlinesRequestRange::Limit(value as u32)
	}
}
impl From<u8> for KlinesRequestRange {
	fn from(value: u8) -> Self {
		KlinesRequestRange::Limit(value as u32)
	}
}
impl From<(DateTime<Utc>, DateTime<Utc>)> for KlinesRequestRange {
	fn from(value: (DateTime<Utc>, DateTime<Utc>)) -> Self {
		KlinesRequestRange::StartEnd { start: value.0, end: Some(value.1) }
	}
}
impl From<(i64, i64)> for KlinesRequestRange {
	fn from(value: (i64, i64)) -> Self {
		KlinesRequestRange::StartEnd {
			start: DateTime::from_timestamp_millis(value.0).unwrap(),
			end: Some(DateTime::from_timestamp_millis(value.1).unwrap()),
		}
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
	pub pairs: HashMap<Pair, PairInfo>,
}
#[derive(Clone, Debug, Default)]
pub struct PairInfo {
	pub price_precision: u8,
}
