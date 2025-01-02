pub mod futures;
use color_eyre::eyre::Result;
use v_exchanges_adapters::{Client, binance};
use v_utils::trades::{Asset, Pair, Timeframe};

use crate::core::{AssetBalance, Exchange, Klines};

#[derive(Clone, Debug, Default)]
pub struct Binance(pub Client);
impl Binance {
	pub fn new() -> Self {
		Self(Client::new())
	}
}
impl std::ops::Deref for Binance {
	type Target = Client;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}
impl std::ops::DerefMut for Binance {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}

//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
impl Exchange for Binance {
	async fn futures_klines(&self, symbol: Pair, tf: Timeframe, limit: u32, start_time: Option<u64>, end_time: Option<u64>) -> Result<Klines> {
		futures::market::klines(&self.0, symbol, tf, limit, start_time, end_time).await
	}

	async fn futures_price(&self, symbol: Pair) -> Result<f64> {
		futures::market::price(&self.0, symbol).await
	}

	async fn futures_asset_balance(&self, asset: Asset) -> Result<AssetBalance> {
		futures::account::asset_balance(&self.0, asset).await
	}

	async fn futures_balances(&self) -> Result<Vec<AssetBalance>> {
		futures::account::balances(&self.0).await
	}

	//DO: async fn balance(&self,
}
