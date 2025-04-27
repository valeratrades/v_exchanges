pub mod data; // interfaced with directly, not through `Exchange` trait, thus must be public.
mod perp;
use std::collections::BTreeMap;
mod market;
mod spot;
mod ws;
use adapters::{Client, binance::BinanceOption, generics::ws::WsError};
use secrecy::SecretString;
use tokio::sync::mpsc;
use v_utils::trades::{Asset, Pair, Timeframe};

use crate::{
	AssetBalance, Balances, Exchange, ExchangeError, ExchangeInfo, ExchangeName, ExchangeResult, Klines, MethodError, RequestRange,
	core::{Instrument, Symbol},
};

#[derive(Clone, Debug, Default, derive_more::Deref, derive_more::DerefMut)]
pub struct Binance(pub Client);

#[async_trait::async_trait]
impl Exchange for Binance {
	fn name(&self) -> ExchangeName {
		ExchangeName::Binance
	}

	fn __client_mut(&mut self) -> &mut Client {
		&mut self.0
	}

	fn auth(&mut self, pubkey: String, secret: SecretString) {
		self.update_default_option(BinanceOption::Pubkey(pubkey));
		self.update_default_option(BinanceOption::Secret(secret));
	}

	fn set_recv_window(&mut self, recv_window: u16) {
		self.update_default_option(BinanceOption::RecvWindow(recv_window));
	}

	async fn exchange_info(&self, instrument: Instrument) -> ExchangeResult<ExchangeInfo> {
		match instrument {
			Instrument::Perp => perp::general::exchange_info(self).await,
			_ => unimplemented!(),
		}
	}

	async fn klines(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Klines> {
		match symbol.instrument {
			Instrument::Spot | Instrument::Margin => market::klines(self, symbol, tf.try_into()?, range).await,
			Instrument::Perp => market::klines(self, symbol, tf.try_into()?, range).await,
			_ => Err(ExchangeError::Method(MethodError::MethodNotImplemented {
				exchange: self.name(),
				instrument: symbol.instrument,
			})),
		}
	}

	async fn prices(&self, pairs: Option<Vec<Pair>>, instrument: Instrument) -> ExchangeResult<BTreeMap<Pair, f64>> {
		match instrument {
			Instrument::Spot | Instrument::Margin => spot::market::prices(self, pairs).await,
			Instrument::Perp => perp::market::prices(self, pairs).await,
			_ => Err(ExchangeError::Method(MethodError::MethodNotImplemented { exchange: self.name(), instrument })),
		}
	}

	async fn price(&self, symbol: Symbol) -> ExchangeResult<f64> {
		match symbol.instrument {
			Instrument::Spot | Instrument::Margin => spot::market::price(self, symbol.pair).await,
			Instrument::Perp => perp::market::price(self, symbol.pair).await,
			_ => Err(ExchangeError::Method(MethodError::MethodNotImplemented {
				exchange: self.name(),
				instrument: symbol.instrument,
			})),
		}
	}

	async fn asset_balance(&self, asset: Asset, recv_window: Option<u16>, instrument: Instrument) -> ExchangeResult<AssetBalance> {
		match instrument {
			Instrument::Perp => perp::account::asset_balance(self, asset, recv_window).await,
			_ => unimplemented!(),
		}
	}

	async fn balances(&self, recv_window: Option<u16>, instrument: Instrument) -> ExchangeResult<Balances> {
		match instrument {
			Instrument::Perp => {
				let prices = self.prices(None, instrument).await?;
				perp::account::balances(self, recv_window, &prices).await
			}
			_ => unimplemented!(),
		}
	}

	async fn ws_trades(&self, symbol: Symbol) -> ExchangeResult<mpsc::Receiver<Result<crate::core::TradeEvent, WsError>>> {
		match symbol.instrument {
			Instrument::Perp => Ok(ws::trades(self, symbol).await),
			Instrument::Spot | Instrument::Margin => Ok(ws::trades(self, symbol).await),
			_ => Err(ExchangeError::Method(MethodError::MethodNotImplemented {
				exchange: self.name(),
				instrument: symbol.instrument,
			})),
		}
	}
}

crate::define_provider_timeframe!(
	BinanceTimeframe,
	[
		"1s", "5s", "15s", "30s", "1m", "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h", "8h", "12h", "1d", "3d", "1w", "1M"
	],
	"Binance"
);
