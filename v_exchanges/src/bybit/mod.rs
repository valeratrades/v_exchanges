mod account;
mod market;

use adapters::bybit::BybitOption;
use color_eyre::eyre::Result;
use derive_more::derive::{Deref, DerefMut};
use v_exchanges_adapters::Client;
use v_utils::trades::{Asset, Pair, Timeframe};

use crate::core::{AssetBalance, Exchange, Klines, KlinesRequestRange};

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct Bybit(pub Client);

//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
impl Exchange for Bybit {
	fn auth<S: Into<String>>(&mut self, key: S, secret: S) {
		self.update_default_option(BybitOption::Key(key.into()));
		self.update_default_option(BybitOption::Secret(secret.into()));
	}

	async fn futures_klines(&self, symbol: Pair, tf: Timeframe, range: KlinesRequestRange) -> Result<Klines> {
		market::klines(&self.0, symbol, tf, range).await
	}

	async fn futures_price(&self, symbol: Pair) -> Result<f64> {
		market::price(&self.0, symbol).await
	}

	async fn futures_asset_balance(&self, asset: Asset) -> Result<AssetBalance> {
		account::asset_balance(&self.0, asset).await
	}

	async fn futures_balances(&self) -> Result<Vec<AssetBalance>> {
		account::balances(&self.0).await
	}

	//DO: async fn balance(&self,
}
