mod account;
mod market;

use std::collections::BTreeMap;

use adapters::bybit::BybitOption;
use derive_more::{
	Display, FromStr,
	derive::{Deref, DerefMut},
};
use eyre::Result;
use v_exchanges_adapters::Client;
use v_utils::trades::{Asset, Pair, Timeframe};

use crate::{
	Balances,
	core::{AbsMarket, AssetBalance, Exchange, ExchangeInfo, Klines, RequestRange, WrongExchangeError},
};

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct Bybit {
	#[deref_mut]
	#[deref]
	client: Client,
	source_market: Option<AbsMarket>,
}

//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
#[async_trait::async_trait]
impl Exchange for Bybit {
	fn source_market(&self) -> AbsMarket {
		self.source_market.unwrap()
	}

	fn auth(&mut self, key: String, secret: String) {
		self.update_default_option(BybitOption::Key(key));
		self.update_default_option(BybitOption::Secret(secret));
	}

	async fn exchange_info(&self, am: AbsMarket) -> Result<ExchangeInfo> {
		match am {
			AbsMarket::Bybit(_) => todo!(),
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn klines(&self, pair: Pair, tf: Timeframe, range: RequestRange, am: AbsMarket) -> Result<Klines> {
		match am {
			AbsMarket::Bybit(m) => match m {
				Market::Linear => market::klines(&self.client, pair, tf, range, am).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn price(&self, pair: Pair, am: AbsMarket) -> Result<f64> {
		match am {
			AbsMarket::Bybit(m) => match m {
				Market::Linear => market::price(&self.client, pair).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn prices(&self, pairs: Option<Vec<Pair>>, am: AbsMarket) -> Result<BTreeMap<Pair, f64>> {
		match am {
			AbsMarket::Bybit(_) => todo!(),
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn asset_balance(&self, asset: Asset, am: AbsMarket) -> Result<AssetBalance> {
		match am {
			AbsMarket::Bybit(m) => match m {
				Market::Linear => account::asset_balance(&self.client, asset).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn balances(&self, am: AbsMarket) -> Result<Balances> {
		match am {
			AbsMarket::Bybit(m) => match m {
				Market::Linear => account::balances(&self.client).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
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
	fn client(&self, source_market: AbsMarket) -> Box<dyn Exchange> {
		Box::new(Bybit {
			source_market: Some(source_market),
			..Default::default()
		})
	}

	fn abs_market(&self) -> AbsMarket {
		AbsMarket::Bybit(self.clone())
	}
}
