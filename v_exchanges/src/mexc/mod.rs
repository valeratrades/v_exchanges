mod account;

use adapters::mexc::MexcOption;
use derive_more::{
	Display, FromStr,
	derive::{Deref, DerefMut},
};
use eyre::Result;
use v_exchanges_adapters::Client;
use v_utils::trades::{Asset, Pair, Timeframe};

use crate::core::{AbsMarket, AssetBalance, Exchange, ExchangeInfo, Klines, RequestRange, WrongExchangeError};

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct Mexc {
	#[deref_mut]
	#[deref]
	client: Client,
	source_market: AbsMarket,
}

//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
#[async_trait::async_trait]
impl Exchange for Mexc {
	fn source_market(&self) -> AbsMarket {
		self.source_market
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
			AbsMarket::Mexc(m) => match m {
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn price(&self, pair: Pair, am: AbsMarket) -> Result<f64> {
		match am {
			AbsMarket::Mexc(m) => match m {
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn prices(&self, pairs: Option<Vec<Pair>>, am: AbsMarket) -> Result<Vec<(Pair, f64)>> {
		match am {
			AbsMarket::Mexc(_) => todo!(),
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

	async fn balances(&self, am: AbsMarket) -> Result<Vec<AssetBalance>> {
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
	fn client(&self) -> Box<dyn Exchange> {
		Box::new(Mexc::default())
	}
}
