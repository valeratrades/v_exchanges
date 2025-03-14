mod account;
mod market;

use std::collections::BTreeMap;

use adapters::bybit::BybitOption;
use derive_more::{
	Display, FromStr,
	derive::{Deref, DerefMut},
};
use secrecy::SecretString;
use v_exchanges_adapters::Client;
use v_utils::trades::{Asset, Pair, Timeframe};

use crate::{
	Balances, ExchangeResult, UnsupportedTimeframeError,
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

	fn __client(&self) -> &Client {
		&self.client
	}

	fn __client_mut(&mut self) -> &mut Client {
		&mut self.client
	}

	fn auth(&mut self, key: String, secret: SecretString) {
		self.update_default_option(BybitOption::Key(key));
		self.update_default_option(BybitOption::Secret(secret));
	}

	fn set_recv_window(&mut self, recv_window: u16) {
		self.update_default_option(BybitOption::RecvWindow(recv_window));
	}

	async fn exchange_info(&self, am: AbsMarket) -> ExchangeResult<ExchangeInfo> {
		match am {
			AbsMarket::Bybit(_) => todo!(),
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn klines(&self, pair: Pair, tf: Timeframe, range: RequestRange, am: AbsMarket) -> ExchangeResult<Klines> {
		match am {
			AbsMarket::Bybit(m) => match m {
				Market::Linear => market::klines(&self.client, pair, tf.try_into()?, range, am).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn price(&self, pair: Pair, am: AbsMarket) -> ExchangeResult<f64> {
		match am {
			AbsMarket::Bybit(m) => match m {
				Market::Linear => market::price(&self.client, pair).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn prices(&self, pairs: Option<Vec<Pair>>, am: AbsMarket) -> ExchangeResult<BTreeMap<Pair, f64>> {
		match am {
			AbsMarket::Bybit(_) => todo!(),
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn asset_balance(&self, asset: Asset, recv_window: Option<u16>, am: AbsMarket) -> ExchangeResult<AssetBalance> {
		match am {
			AbsMarket::Bybit(m) => match m {
				Market::Linear => account::asset_balance(&self.client, asset, recv_window).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn balances(&self, recv_window: Option<u16>, am: AbsMarket) -> ExchangeResult<Balances> {
		match am {
			AbsMarket::Bybit(m) => match m {
				Market::Linear => account::balances(&self.client, recv_window).await,
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
		AbsMarket::Bybit(*self)
	}
}

crate::define_provider_timeframe!(BybitTimeframe, ["1", "3", "5", "15", "30", "60", "120", "240", "360", "720", "D", "W", "M"], "Bybit");
