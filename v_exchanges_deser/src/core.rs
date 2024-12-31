use chrono::{DateTime, Utc};
use color_eyre::eyre::Result;
use tokio::sync::mpsc;
use v_exchanges_adapters::traits::HandlerOptions;
use v_utils::trades::{Kline, Pair, Timeframe};

#[derive(Clone, Debug, Default, derive_new::new, Copy)]
pub struct Oi {
	pub lsr: f64,
	pub total: f64,
	pub timestamp: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, derive_new::new)]
pub struct Klines {
	pub v: Vec<Kline>,
	pub tf: Timeframe,
	/// Doesn't have to be synchronized with klines; each track has its own timestamps.
	pub oi: Vec<Oi>,
}

pub trait Exchange<O: HandlerOptions> {
	//? should I have Self::Pair too? Like to catch the non-existent ones immediately? Although this would increase the error surface on new listings.
	fn klines(&self, symbol: Pair, tf: Timeframe, limit: Option<u32>, start_time: Option<u64>, end_time: Option<u64>) -> impl std::future::Future<Output = Result<Klines>> + Send;
	fn price(&self, symbol: Pair) -> impl std::future::Future<Output = Result<f64>> + Send;

	// Defined in terms of actors
	//TODO!!!: fn spawn_klines_listener(&self, symbol: Pair, tf: Timeframe) -> mpsc::Receiver<Kline>;

	//DO: balances
	// balances are defined for each margin type: [futures_balance, spot_balance, margin_balance], but note that on some exchanges, (like bybit), some of these may point to the same exact call
	// to negate confusion could add a `total_balance` endpoint

	//? could implement many things that are _explicitly_ combinatorial. I can imagine several cases, where knowing that say the specified limit for the klines is wayyy over the max and that you may be opting into a long wait by calling it, could be useful.
}
