mod account;
mod market;

use std::collections::BTreeMap;

use adapters::mexc::MexcOption;
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
pub struct Mexc {
	#[deref_mut]
	#[deref]
	client: Client,
	source_market: Option<AbsMarket>,
}

//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
#[async_trait::async_trait]
impl Exchange for Mexc {
	fn source_market(&self) -> AbsMarket {
		self.source_market.unwrap()
	}

	fn auth(&mut self, key: String, secret: String) {
		self.update_default_option(MexcOption::Key(key));
		self.update_default_option(MexcOption::Secret(secret));
	}

	async fn exchange_info(&self, am: AbsMarket) -> Result<ExchangeInfo> {
		match am {
			AbsMarket::Mexc(_) => todo!(),
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn klines(&self, pair: Pair, tf: Timeframe, range: RequestRange, am: AbsMarket) -> Result<Klines> {
		match am {
			AbsMarket::Mexc(m) => unimplemented!(),
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn price(&self, pair: Pair, am: AbsMarket) -> Result<f64> {
		match am {
			AbsMarket::Mexc(m) => match m {
				Market::Futures => market::price(self, pair).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn prices(&self, _pairs: Option<Vec<Pair>>, am: AbsMarket) -> Result<BTreeMap<Pair, f64>> {
		match am {
			AbsMarket::Mexc(_) => unimplemented!("Mexc does not have a multi-asset endpoints for futures"),
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn asset_balance(&self, asset: Asset, am: AbsMarket) -> Result<AssetBalance> {
		match am {
			AbsMarket::Mexc(m) => match m {
				Market::Futures => account::asset_balance(&self.client, asset).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn balances(&self, am: AbsMarket) -> Result<Balances> {
		match am {
			AbsMarket::Mexc(m) => match m {
				Market::Futures => account::balances(&self.client).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}
}

#[derive(Debug, Clone, Default, Copy, Display, FromStr)]
pub enum Market {
	#[default]
	Futures,
	Spot,
}
impl crate::core::MarketTrait for Market {
	fn client(&self, source_market: AbsMarket) -> Box<dyn Exchange> {
		Box::new(Mexc {
			source_market: Some(source_market),
			..Default::default()
		})
	}
}
