pub mod data; // interfaced with directly, not through `Exchange` trait, thus must be public.
pub mod perp; // public for accessing order placement and income history functions
use std::collections::BTreeMap;
mod market;
mod spot;
mod ws;
use adapters::{Client, binance::BinanceOption};
use secrecy::SecretString;
use v_utils::trades::{Asset, Pair, Timeframe};

use crate::{
	AssetBalance, Balances, Exchange, ExchangeError, ExchangeInfo, ExchangeName, ExchangeResult, ExchangeStream, Klines, MethodError, RequestRange, Trade,
	core::{Instrument, Symbol},
};

#[derive(Clone, Debug, Default, derive_more::Deref, derive_more::DerefMut)]
pub struct Binance(pub Client);

#[async_trait::async_trait]
impl Exchange for Binance {
	fn name(&self) -> ExchangeName {
		ExchangeName::Binance
	}

	fn auth(&mut self, pubkey: String, secret: SecretString) {
		self.update_default_option(BinanceOption::Pubkey(pubkey));
		self.update_default_option(BinanceOption::Secret(secret));
	}

	fn set_recv_window(&mut self, recv_window: u16) {
		self.update_default_option(BinanceOption::RecvWindow(recv_window));
	}

	async fn exchange_info(&self, instrument: Instrument, recv_window: Option<u16>) -> ExchangeResult<ExchangeInfo> {
		match instrument {
			Instrument::Perp => perp::general::exchange_info(self, recv_window).await,
			_ => unimplemented!(),
		}
	}

	async fn klines(&self, symbol: Symbol, tf: Timeframe, range: RequestRange, recv_window: Option<u16>) -> ExchangeResult<Klines> {
		match symbol.instrument {
			Instrument::Spot | Instrument::Margin => market::klines(self, symbol, tf.try_into()?, range, recv_window).await,
			Instrument::Perp => market::klines(self, symbol, tf.try_into()?, range, recv_window).await,
			_ => Err(ExchangeError::Method(MethodError::MethodNotImplemented {
				exchange: self.name(),
				instrument: symbol.instrument,
			})),
		}
	}

	async fn prices(&self, pairs: Option<Vec<Pair>>, instrument: Instrument, recv_window: Option<u16>) -> ExchangeResult<BTreeMap<Pair, f64>> {
		match instrument {
			Instrument::Spot | Instrument::Margin => spot::market::prices(self, pairs, recv_window).await,
			Instrument::Perp => perp::market::prices(self, pairs, recv_window).await,
			_ => Err(ExchangeError::Method(MethodError::MethodNotImplemented { exchange: self.name(), instrument })),
		}
	}

	async fn price(&self, symbol: Symbol, recv_window: Option<u16>) -> ExchangeResult<f64> {
		match symbol.instrument {
			Instrument::Spot | Instrument::Margin => spot::market::price(self, symbol.pair, recv_window).await,
			Instrument::Perp => perp::market::price(self, symbol.pair, recv_window).await,
			_ => Err(ExchangeError::Method(MethodError::MethodNotImplemented {
				exchange: self.name(),
				instrument: symbol.instrument,
			})),
		}
	}

	async fn open_interest(&self, symbol: Symbol, tf: Timeframe, range: RequestRange, recv_window: Option<u16>) -> ExchangeResult<Vec<crate::core::OpenInterest>> {
		match symbol.instrument {
			Instrument::Perp => market::open_interest(self, symbol, tf.try_into()?, range, recv_window).await,
			_ => Err(ExchangeError::Method(MethodError::MethodNotSupported {
				exchange: self.name(),
				instrument: symbol.instrument,
			})),
		}
	}

	async fn asset_balance(&self, asset: Asset, instrument: Instrument, recv_window: Option<u16>) -> ExchangeResult<AssetBalance> {
		match instrument {
			Instrument::Perp => perp::account::asset_balance(self, asset, recv_window).await,
			_ => unimplemented!(),
		}
	}

	async fn balances(&self, instrument: Instrument, recv_window: Option<u16>) -> ExchangeResult<Balances> {
		match instrument {
			Instrument::Perp => {
				let prices = self.prices(None, instrument, recv_window).await?;
				perp::account::balances(self, recv_window, &prices).await
			}
			_ => unimplemented!(),
		}
	}

	fn ws_trades(&self, pairs: Vec<Pair>, instrument: Instrument) -> Result<Box<dyn ExchangeStream<Item = Trade>>, ExchangeError> {
		match instrument {
			Instrument::Perp | Instrument::Spot | Instrument::Margin => {
				let connection = ws::TradesConnection::new(self, pairs, instrument)?;
				Ok(Box::new(connection))
			}
			_ => Err(ExchangeError::Method(MethodError::MethodNotImplemented { exchange: self.name(), instrument })),
		}
	}
}

crate::define_provider_timeframe!(
	BinanceTimeframe,
	[
		"1s", "5s", "15s", "30s", "1m", "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h", "8h", "12h", "1d", "3d", "1w", "1M"
	]
);
