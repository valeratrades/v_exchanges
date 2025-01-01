pub use v_exchanges_adapters::Client; // re-export //? not sure if this is the correct place to do this

pub mod account;

use color_eyre::eyre::Result;
use v_exchanges_adapters::bybit;
use v_utils::trades::{Asset, Pair, Timeframe};

use crate::core::{AssetBalance, Exchange, Klines};

//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
impl Exchange<bybit::BybitOptions> for Client {
	async fn futures_klines(&self, symbol: Pair, tf: Timeframe, limit: u32, start_time: Option<u64>, end_time: Option<u64>) -> Result<Klines> {
		//futures::market::klines(&self, symbol, tf, limit, start_time, end_time).await
		todo!();
	}

	async fn futures_price(&self, symbol: Pair) -> Result<f64> {
		//futures::market::price(&self, symbol).await
		todo!();
	}

	async fn futures_asset_balance(&self, asset: Asset) -> Result<AssetBalance> {
		//futures::account::asset_balance(&self, asset).await
		todo!();
	}

	async fn futures_balances(&self) -> Result<Vec<AssetBalance>> {
		account::balances(&self).await
	}

	//DO: async fn balance(&self,
}
