pub mod data; // interfaced with directly, not through `Exchange` trait, thus must be public.
mod futures;
use std::collections::BTreeMap;
mod market;
mod spot;
use adapters::binance::BinanceOption;
use derive_more::{Deref, DerefMut};
use eyre::Result;
use secrecy::SecretString;
use v_exchanges_adapters::Client;
use v_utils::trades::{Asset, Pair, Timeframe};

use crate::{AbsMarket, AssetBalance, Balances, Exchange, ExchangeInfo, ExchangeResult, Klines, RequestRange, UnsupportedTimeframeError, WrongExchangeError};

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct Binance {
	#[deref_mut]
	#[deref]
	client: Client,
	source_market: Option<AbsMarket>,
}

//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
#[async_trait::async_trait]
impl Exchange for Binance {
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
		self.update_default_option(BinanceOption::Key(key));
		self.update_default_option(BinanceOption::Secret(secret));
	}

	fn set_recv_window(&mut self, recv_window: u16) {
		self.update_default_option(BinanceOption::RecvWindow(recv_window));
	}

	async fn exchange_info(&self, am: AbsMarket) -> ExchangeResult<ExchangeInfo> {
		match am {
			AbsMarket::Binance(m) => match m {
				Market::Futures => futures::general::exchange_info(&self.client).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn klines(&self, pair: Pair, tf: Timeframe, range: RequestRange, am: AbsMarket) -> ExchangeResult<Klines> {
		match am {
			AbsMarket::Binance(m) => market::klines(&self.client, pair, tf, range, m).await,
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn prices(&self, pairs: Option<Vec<Pair>>, am: AbsMarket) -> ExchangeResult<BTreeMap<Pair, f64>> {
		match am {
			AbsMarket::Binance(m) => match m {
				Market::Spot => spot::market::prices(&self.client, pairs).await,
				Market::Futures => futures::market::prices(&self.client, pairs).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn price(&self, pair: Pair, am: AbsMarket) -> ExchangeResult<f64> {
		match am {
			AbsMarket::Binance(m) => match m {
				Market::Spot => spot::market::price(&self.client, pair).await,
				Market::Futures => futures::market::price(&self.client, pair).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn asset_balance(&self, asset: Asset, recv_window: Option<u16>, am: AbsMarket) -> ExchangeResult<AssetBalance> {
		match am {
			AbsMarket::Binance(m) => match m {
				Market::Futures => futures::account::asset_balance(self, asset, recv_window).await,
				_ => unimplemented!(),
			},
			_ => Err(WrongExchangeError::new(self.exchange_name(), am).into()),
		}
	}

	async fn balances(&self, recv_window: Option<u16>, am: AbsMarket) -> ExchangeResult<Balances> {
		match am {
			AbsMarket::Binance(m) => match m {
				Market::Futures => {
					let prices = self.prices(None, am).await?;
					futures::account::balances(&self.client, recv_window, &prices).await
				}
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
	fn client(&self, source_market: AbsMarket) -> Box<dyn Exchange> {
		Box::new(Binance {
			source_market: Some(source_market),
			..Default::default()
		})
	}

	fn abs_market(&self) -> AbsMarket {
		AbsMarket::Binance(*self)
	}
}

static ALLOWED_TFS_BINANCE: [&str; 13] = ["1", "3", "5", "15", "30", "60", "120", "240", "360", "720", "D", "W", "M"];
#[derive(Debug, Clone, Default, Copy, derive_more::Deref, derive_more::DerefMut)]
pub struct BinanceTimeframe(Timeframe);
impl BinanceTimeframe {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		todo!()
	}
}

impl TryFrom<Timeframe> for BinanceTimeframe {
	type Error = UnsupportedTimeframeError;

	fn try_from(t: Timeframe) -> Result<Self, Self::Error> {
		let mut buf = String::new();
		let mut formatter = core::fmt::Formatter::new(&mut buf, core::fmt::FormattingOptions::new());

		let maybe_ok = BinanceTimeframe(t);
		// hacky, but saves dev-time
		match core::fmt::Display::fmt(&maybe_ok, &mut formatter) {
			Ok(_) => Ok(Self(t)),
			Err(_) => Err(UnsupportedTimeframeError::new(t, ALLOWED_TFS_BINANCE.iter().map(Timeframe::from).collect())),
		}
	}
}
impl std::fmt::Display for BinanceTimeframe {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		todo!()
	}
}
