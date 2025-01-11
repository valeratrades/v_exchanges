mod account;
mod market;

use derive_more::{Display, FromStr};
use adapters::bybit::BybitOption;
use derive_more::derive::{Deref, DerefMut};
use eyre::Result;
use v_exchanges_adapters::Client;
use v_utils::trades::{Asset, Pair, Timeframe};

use crate::core::{AssetBalance, Exchange, Klines, KlinesRequestRange};

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct Bybit(pub Client);

//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
impl Exchange for Bybit {
	type M = Market;

	fn auth<S: Into<String>>(&mut self, key: S, secret: S) {
		self.update_default_option(BybitOption::Key(key.into()));
		self.update_default_option(BybitOption::Secret(secret.into()));
	}

	async fn klines(&self, pair: Pair, tf: Timeframe, range: KlinesRequestRange, m: Self::M) -> Result<Klines> {
		match m {
			Market::Linear => market::klines(&self.0, pair, tf, range).await,
			_ => unimplemented!(),
		}
	}

	async fn price(&self, pair: Pair, m: Self::M) -> Result<f64> {
		match m {
			Market::Linear => market::price(&self.0, pair).await,
			_ => unimplemented!(),
		}
	}

	async fn prices(&self, pairs: Option<Vec<Pair>>, m: Self::M) -> Result<Vec<(Pair, f64)>> {
		todo!();
	}

	async fn asset_balance(&self, asset: Asset, m: Self::M) -> Result<AssetBalance> {
		match m {
		Market::Linear => account::asset_balance(&self.0, asset).await,
			_ => unimplemented!(),
		}
	}

	async fn balances(&self, m: Self::M) -> Result<Vec<AssetBalance>> {
		match m{
			Market::Linear => account::balances(&self.0).await,
			_ => unimplemented!(),
		}
	}
}


#[derive(Debug, Clone, Default, Copy, Display, FromStr)]
pub enum Market {
	#[default]
	Linear,
	Spot,
	Inverse,
}
impl crate::core::MarketTrait for Market {
	type Client = Bybit;
	fn client(&self) -> Bybit {
		Bybit::default()
	}
}
