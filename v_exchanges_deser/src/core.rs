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
	// Defined in terms of actors
	//TODO!!!: fn spawn_klines_listener(&self, symbol: Pair, tf: Timeframe) -> mpsc::Receiver<Kline>;
}
