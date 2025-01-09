mod data;
mod futures;
mod market;
mod spot;
use adapters::binance::BinanceOption;
use derive_more::{Deref, DerefMut};
use eyre::Result;
use v_exchanges_adapters::Client;
use v_utils::trades::{Asset, Pair, Timeframe};

use crate::core::{AssetBalance, Exchange, Klines, KlinesRequestRange};

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct Binance(pub Client);

//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
impl Exchange for Binance {
	fn auth<S: Into<String>>(&mut self, key: S, secret: S) {
		self.update_default_option(BinanceOption::Key(key.into()));
		self.update_default_option(BinanceOption::Secret(secret.into()));
	}

	async fn spot_klines(&self, pair: Pair, tf: Timeframe, range: KlinesRequestRange) -> Result<Klines> {
		market::klines(&self.0, pair, tf, range, Market::Spot).await
	}

	async fn spot_prices(&self, pairs: Option<Vec<Pair>>) -> Result<Vec<(Pair, f64)>> {
		spot::market::prices(&self.0, pairs).await
	}

	async fn spot_price(&self, pair: Pair) -> Result<f64> {
		spot::market::price(&self.0, pair).await
	}

	async fn futures_klines(&self, pair: Pair, tf: Timeframe, range: KlinesRequestRange) -> Result<Klines> {
		market::klines(&self.0, pair, tf, range, Market::Futures).await
	}

	async fn futures_price(&self, pair: Pair) -> Result<f64> {
		futures::market::price(&self.0, pair).await
	}

	async fn futures_asset_balance(&self, asset: Asset) -> Result<AssetBalance> {
		futures::account::asset_balance(&self.0, asset).await
	}

	async fn futures_balances(&self) -> Result<Vec<AssetBalance>> {
		futures::account::balances(&self.0).await
	}
}

#[derive(Clone, Debug, Default, derive_new::new, Copy)]
pub enum Market {
	#[default]
	Futures,
	Spot,
	Margin,
}
