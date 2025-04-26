pub mod data; // interfaced with directly, not through `Exchange` trait, thus must be public.
mod perp;
use std::collections::BTreeMap;
mod market;
mod spot;
mod ws;
use adapters::{Client, binance::BinanceOption, generics::ws::WsError};
use derive_more::{Deref, DerefMut};
use secrecy::SecretString;
use tokio::sync::mpsc;
use v_utils::trades::{Asset, Pair, Timeframe};

use crate::{
	AbsMarket, AssetBalance, Balances, Exchange, ExchangeInfo, ExchangeResult, Klines, RequestRange, WrongExchangeError,
	types::{Instrument, Symbol},
};

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
	fn __client(&self) -> &Client {
		&self.client
	}

	fn __client_mut(&mut self) -> &mut Client {
		&mut self.client
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
			Instrument::Perp => perp::general::exchange_info(&self.client).await,
			_ => unimplemented!(),
		}
	}

	async fn klines(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Klines> {
		match symbol.instrument {
			Instrument::Spot | Instrument::Margin => market::klines(&self.client, symbol.pair, tf.try_into()?, range, Market::Spot).await,
			Instrument::Perp => market::klines(&self.client, symbol.pair, tf.try_into()?, range, Market::Perp).await,
			_ => unimplemented!(),
		}
	}

	async fn prices(&self, pairs: Option<Vec<Pair>>, instrument: Instrument) -> ExchangeResult<BTreeMap<Pair, f64>> {
		match instrument {
			Instrument::Spot => spot::market::prices(&self.client, pairs).await,
			Instrument::Perp => perp::market::prices(&self.client, pairs).await,
			_ => unimplemented!(),
		}
	}

	async fn price(&self, symbol: Symbol) -> ExchangeResult<f64> {
		match symbol.instrument {
			Instrument::Spot => spot::market::price(&self.client, symbol.pair).await,
			Instrument::Perp => perp::market::price(&self.client, symbol.pair).await,
			_ => unimplemented!(),
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
				perp::account::balances(&self.client, recv_window, &prices).await
			}
			_ => unimplemented!(),
		}
	}

	async fn ws_trades(&self, symbol: Symbol) -> ExchangeResult<mpsc::Receiver<Result<crate::ws_types::TradeEvent, WsError>>> {
		match symbol.instrument {
			Instrument::Perp => Ok(ws::trades(&self.client, symbol.pair, Market::Perp).await),
			Instrument::Spot | Instrument::Margin => Ok(ws::trades(&self.client, symbol.pair, Market::Spot).await),
			_ => unimplemented!(),
		}
	}
}

//TODO: add `Futures`, `Perpetual`, `Perp`, `Perps`, etc options as possible source deff strings.
#[derive(Clone, Copy, Debug, Default, derive_more::Display, derive_more::FromStr)]
#[non_exhaustive]
pub enum Market {
	#[default]
	Perp,
	Spot,
	/// Margin. Name shortened for alignment, following Tiger Style
	Marg,
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

crate::define_provider_timeframe!(
	BinanceTimeframe,
	[
		"1s", "5s", "15s", "30s", "1m", "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h", "8h", "12h", "1d", "3d", "1w", "1M"
	],
	"Binance"
);
