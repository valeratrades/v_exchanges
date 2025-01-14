mod data;
mod futures;
mod market;
mod spot;
use adapters::binance::BinanceOption;
use derive_more::{Deref, DerefMut};
use eyre::Result;
use v_exchanges_adapters::Client;
use v_utils::trades::{Asset, Pair, Timeframe};

use crate::core::{AbsMarket, AssetBalance, Exchange, ExchangeInfo, Klines, RequestRange, WrongExchangeError};

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct Binance(pub Client);

//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
#[async_trait::async_trait]
impl Exchange for Binance {
	fn exchange_name(&self) -> &'static str {
		"Binance"
	}

	fn auth(&mut self, key: String, secret: String) {
		self.update_default_option(BinanceOption::Key(key));
		self.update_default_option(BinanceOption::Secret(secret));
	}

	async fn exchange_info(&self, am: AbsMarket) -> Result<ExchangeInfo> {
		match am {
			AbsMarket::Binance(m) => match m {
				Market::Futures => futures::general::exchange_info(&self.0).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn klines(&self, pair: Pair, tf: Timeframe, range: RequestRange, am: AbsMarket) -> Result<Klines> {
		match am {
			AbsMarket::Binance(m) => market::klines(&self.0, pair, tf, range, m).await,
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn prices(&self, pairs: Option<Vec<Pair>>, am: AbsMarket) -> Result<Vec<(Pair, f64)>> {
		match am {
			AbsMarket::Binance(m) => match m {
				Market::Spot => spot::market::prices(&self.0, pairs).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn price(&self, pair: Pair, am: AbsMarket) -> Result<f64> {
		match am {
			AbsMarket::Binance(m) => match m {
				Market::Spot => spot::market::price(&self.0, pair).await,
				Market::Futures => futures::market::price(&self.0, pair).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn asset_balance(&self, asset: Asset, am: AbsMarket) -> Result<AssetBalance> {
		match am {
			AbsMarket::Binance(m) => match m {
				Market::Futures => futures::account::asset_balance(self, asset).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn balances(&self, am: AbsMarket) -> Result<Vec<AssetBalance>> {
		match am {
			AbsMarket::Binance(m) => match m {
				Market::Futures => futures::account::balances(&self.0).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}
}

#[derive(Debug, Clone, Default, Copy, derive_more::Display, derive_more::FromStr)]
pub enum Market {
	#[default]
	Futures,
	Spot,
	Margin,
}
impl crate::core::MarketTrait for Market {
	fn client(&self) -> Box<dyn Exchange> {
		Box::new(Binance::default())
	}

	fn fmt_abs(&self) -> String {
		format!("Binance/{self}")
	}
}
