mod data;
mod futures;
mod market;
mod spot;
use adapters::binance::BinanceOption;
use derive_more::{Deref, DerefMut, Display, FromStr};
use eyre::Result;
use v_exchanges_adapters::Client;
use v_utils::trades::{Asset, Pair, Timeframe};

use crate::core::{AssetBalance, Exchange, Klines, KlinesRequestRange};

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct Binance(pub Client);

//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
impl Exchange for Binance {
	type M = Market;

	fn auth<S: Into<String>>(&mut self, key: S, secret: S) {
		self.update_default_option(BinanceOption::Key(key.into()));
		self.update_default_option(BinanceOption::Secret(secret.into()));
	}

	async fn klines(&self, pair: Pair, tf: Timeframe, range: KlinesRequestRange, m: Self::M) -> Result<Klines> {
		market::klines(&self.0, pair, tf, range, m).await
	}

	async fn prices(&self, pairs: Option<Vec<Pair>>, m: Self::M) -> Result<Vec<(Pair, f64)>> {
		match m {
			Market::Spot => spot::market::prices(&self.0, pairs).await,
			_ => unimplemented!(),
		}
	}

	async fn price(&self, pair: Pair, m: Self::M) -> Result<f64> {
		match m {
			Market::Spot => spot::market::price(&self.0, pair).await,
			Market::Futures => futures::market::price(&self.0, pair).await,
			_ => unimplemented!(),
		}
	}

	async fn asset_balance(&self, asset: Asset, m: Self::M) -> Result<AssetBalance> {
		match m {
			Market::Futures => futures::account::asset_balance(self, asset).await,
			_ => unimplemented!(),
		}
	}

	async fn balances(&self, m: Self::M) -> Result<Vec<AssetBalance>> {
		match m {
			Market::Futures => futures::account::balances(&self.0).await,
			_ => unimplemented!(),
		}
	}
}

#[derive(Debug, Clone, Default, Copy, Display, FromStr)]
pub enum Market {
	#[default]
	Futures,
	Spot,
	Margin,
}
impl crate::core::MarketTrait for Market {
	type Client = Binance;

	fn client(&self) -> Binance {
		Binance::default()
	}
}
