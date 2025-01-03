pub mod futures;
use color_eyre::eyre::Result;
use derive_more::{Deref, DerefMut};
use v_exchanges_adapters::{Client, binance};
use v_utils::{
	macros::WrapNew,
	trades::{Asset, Pair, Timeframe},
};

use crate::core::{AssetBalance, Exchange, Klines, KlinesRequestRange};

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct Binance(pub Client);

//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
impl Exchange for Binance {
	async fn futures_klines(&self, symbol: Pair, tf: Timeframe, range: KlinesRequestRange) -> Result<Klines> {
		futures::market::klines(&self.0, symbol, tf, range).await
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
