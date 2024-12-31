pub mod futures;
use color_eyre::eyre::Result;
use futures::market::klines;
pub use v_exchanges_adapters::Client; // re-export
use v_exchanges_adapters::binance;
use v_utils::trades::{Pair, Timeframe};

use crate::core::{Exchange, Klines};

impl Exchange<binance::BinanceOptions> for Client {
	async fn klines(&self, symbol: Pair, tf: Timeframe, limit: Option<u32>, start_time: Option<u64>, end_time: Option<u64>) -> Result<Klines> {
		klines(&self, symbol, tf, limit, start_time, end_time).await
	}
}
