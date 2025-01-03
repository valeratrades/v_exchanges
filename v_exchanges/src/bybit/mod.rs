mod account;
mod market;

use color_eyre::eyre::Result;
use derive_more::derive::{Deref, DerefMut};
use v_exchanges_adapters::{Client, bybit};
use v_utils::trades::{Asset, Pair, Timeframe};

use crate::core::{AssetBalance, Exchange, Klines, KlinesRequestRange};

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct Bybit(pub Client);

//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
impl Exchange for Bybit {
	async fn futures_klines(&self, symbol: Pair, tf: Timeframe, range: KlinesRequestRange) -> Result<Klines> {
		market::klines(&self.0, symbol, tf, range).await
	}

	async fn futures_price(&self, symbol: Pair) -> Result<f64> {
		//futures::market::price(&self.0, symbol).await
		todo!();
	}

	async fn futures_asset_balance(&self, asset: Asset) -> Result<AssetBalance> {
		account::asset_balance(&self.0, asset).await
	}

	async fn futures_balances(&self) -> Result<Vec<AssetBalance>> {
		account::balances(&self.0).await
	}

	//DO: async fn balance(&self,
}
