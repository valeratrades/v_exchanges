use chrono::{DateTime, Utc};
use color_eyre::eyre::{Error, Result};
use tokio::sync::mpsc;
use v_exchanges_adapters::traits::HandlerOptions;
use v_utils::trades::{Asset, Kline, Pair, Timeframe};

//TODO!!!!!!!!!!!!!: klines switch to defining the range via an Enum over either limit either start and end times

pub trait Exchange {
	//? should I have Self::Pair too? Like to catch the non-existent ones immediately? Although this would increase the error surface on new listings.
	fn futures_klines(&self, symbol: Pair, tf: Timeframe, range: KlinesRequestRange) -> impl std::future::Future<Output = Result<Klines>> + Send;
	fn futures_price(&self, symbol: Pair) -> impl std::future::Future<Output = Result<f64>> + Send;

	// Defined in terms of actors
	//TODO!!!: fn spawn_klines_listener(&self, symbol: Pair, tf: Timeframe) -> mpsc::Receiver<Kline>;

	/// balance of a specific asset
	fn futures_asset_balance(&self, asset: Asset) -> impl std::future::Future<Output = Result<AssetBalance>> + Send;
	/// vec of balances of specific assets
	fn futures_balances(&self) -> impl std::future::Future<Output = Result<Vec<AssetBalance>>> + Send;
	//? potentially `total_balance`? Would return precompiled USDT-denominated balance of a (bybit::wallet/binance::account)
	// balances are defined for each margin type: [futures_balance, spot_balance, margin_balance], but note that on some exchanges, (like bybit), some of these may point to the same exact call
	// to negate confusion could add a `total_balance` endpoint

	//? could implement many things that are _explicitly_ combinatorial. I can imagine several cases, where knowing that say the specified limit for the klines is wayyy over the max and that you may be opting into a long wait by calling it, could be useful.
}

// Klines {{{
#[derive(Clone, Debug, Default, Copy)]
pub struct Oi {
	pub lsr: f64,
	pub total: f64,
	pub timestamp: DateTime<Utc>,
}

#[derive(Clone, Debug, Default)]
pub struct Klines {
	pub v: Vec<Kline>,
	pub tf: Timeframe,
	/// Doesn't have to be synchronized with klines; each track has its own timestamps.
	pub oi: Vec<Oi>,
}

//MOVE: v_utils (along with [Klines])
//? not sure what to do about oi here
/// [Kline]s series that is _guaranteed to not have any gaps in kline data_.
#[derive(Clone, Debug, Default)]
pub struct FullKlines(Klines);
impl TryFrom<Klines> for FullKlines {
	type Error = color_eyre::eyre::Report;
	fn try_from(value: Klines) -> Result<Self> {
	  todo!();
	}
}

#[derive(Clone, Debug, Copy)]
pub enum KlinesRequestRange {
	/// Preferred way of defining the range
	StartEnd{start: DateTime<Utc>, end: DateTime<Utc>},
	/// For quick and dirty
	Limit(u32),
}
impl Default for KlinesRequestRange {
	fn default() -> Self {
	  KlinesRequestRange::StartEnd { start: DateTime::default(), end: DateTime::default() }
	}
}
impl From<u32> for KlinesRequestRange {
	fn from(value: u32) -> Self {
	  KlinesRequestRange::Limit(value)
	}
}
impl From<(DateTime<Utc>, DateTime<Utc>)> for KlinesRequestRange {
	fn from(value: (DateTime<Utc>, DateTime<Utc>)) -> Self {
	  KlinesRequestRange::StartEnd { start: value.0, end: value.1 }
	}
}
impl From<(i64, i64)> for KlinesRequestRange {
	fn from(value: (i64, i64)) -> Self {
	  KlinesRequestRange::StartEnd { start: DateTime::from_timestamp_millis(value.0).unwrap(), end: DateTime::from_timestamp_millis(value.1).unwrap()}
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
