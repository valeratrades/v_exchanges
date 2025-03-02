mod account;
mod market;

use std::collections::BTreeMap;

use adapters::mexc::MexcOption;
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

	fn __client(&self) -> &Client {
		&self.client
	}

	fn __client_mut(&mut self) -> &mut Client {
		&mut self.client
	}

	fn auth(&mut self, key: String, secret: SecretString) {
		self.update_default_option(MexcOption::Key(key));
		self.update_default_option(MexcOption::Secret(secret));
	}

	fn set_recv_window(&mut self, recv_window: u16) {
		self.update_default_option(MexcOption::RecvWindow(recv_window));
	}

	async fn exchange_info(&self, am: AbsMarket) -> ExchangeResult<ExchangeInfo> {
		match am {
			AbsMarket::Mexc(_) => todo!(),
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn klines(&self, pair: Pair, tf: Timeframe, range: RequestRange, am: AbsMarket) -> ExchangeResult<Klines> {
		match am {
			AbsMarket::Mexc(m) => unimplemented!(),
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn price(&self, pair: Pair, am: AbsMarket) -> ExchangeResult<f64> {
		match am {
			AbsMarket::Mexc(m) => match m {
				Market::Futures => market::price(self, pair).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn prices(&self, _pairs: Option<Vec<Pair>>, am: AbsMarket) -> ExchangeResult<BTreeMap<Pair, f64>> {
		match am {
			AbsMarket::Mexc(_) => unimplemented!("Mexc does not have a multi-asset endpoints for futures"),
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn asset_balance(&self, asset: Asset, recv_window: Option<u16>, am: AbsMarket) -> ExchangeResult<AssetBalance> {
		match am {
			AbsMarket::Mexc(m) => match m {
				Market::Futures => account::asset_balance(&self.client, asset, recv_window).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn balances(&self, recv_window: Option<u16>, am: AbsMarket) -> ExchangeResult<Balances> {
		match am {
			AbsMarket::Mexc(m) => match m {
				Market::Futures => account::balances(&self.client, recv_window).await,
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

	fn abs_market(&self) -> AbsMarket {
		AbsMarket::Mexc(*self)
	}
}

static TFS_MEXC: [&str; 9] = ["1m", "5m", "15m", "30m", "60m", "4h", "1d", "1W", "1M"];
#[derive(Debug, Clone, Default, Copy, derive_more::Deref, derive_more::DerefMut)]
pub struct MexcTimeframe(Timeframe);
impl std::fmt::Display for MexcTimeframe {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let s = self
			.0
			.try_as_predefined(&TFS_MEXC)
			.expect("We can't create a MexcTimeframe object if that doesn't succeed in the first place");
		write!(f, "{s}")
	}
}
impl TryFrom<Timeframe> for MexcTimeframe {
	type Error = UnsupportedTimeframeError;

	fn try_from(t: Timeframe) -> Result<Self, Self::Error> {
		match t.try_as_predefined(&TFS_MEXC) {
			Some(_) => Ok(Self(t)),
			None => Err(UnsupportedTimeframeError::new(t, TFS_MEXC.iter().map(Timeframe::from).collect())),
		}
	}
}
