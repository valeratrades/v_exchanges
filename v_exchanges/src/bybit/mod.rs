mod account;
mod market;

use adapters::bybit::BybitOption;
use secrecy::SecretString;
use v_exchanges_adapters::Client;
use v_utils::trades::{Asset, Timeframe};

use crate::{
	Balances, ExchangeName, ExchangeResult, Instrument, OpenInterest, Symbol,
	core::{AssetBalance, Exchange, Klines, RequestRange},
};

#[derive(Clone, Debug, Default, derive_more::Deref, derive_more::DerefMut)]
pub struct Bybit(pub Client);

//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
#[async_trait::async_trait]
impl Exchange for Bybit {
	fn name(&self) -> ExchangeName {
		ExchangeName::Bybit
	}

	fn auth(&mut self, pubkey: String, secret: SecretString) {
		self.update_default_option(BybitOption::Pubkey(pubkey));
		self.update_default_option(BybitOption::Secret(secret));
	}

	fn set_recv_window(&mut self, recv_window: u16) {
		self.update_default_option(BybitOption::RecvWindow(recv_window));
	}

	async fn klines(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Klines> {
		match symbol.instrument {
			Instrument::Perp => market::klines(self, symbol, tf.try_into()?, range).await,
			_ => unimplemented!(),
		}
	}

	async fn price(&self, symbol: Symbol) -> ExchangeResult<f64> {
		match symbol.instrument {
			Instrument::Perp => market::price(self, symbol.pair).await,
			_ => unimplemented!(),
		}
	}

	async fn open_interest(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Vec<OpenInterest>> {
		match symbol.instrument {
			Instrument::Perp => market::open_interest(self, symbol, tf.try_into()?, range).await,
			_ => Err(crate::ExchangeError::Method(crate::MethodError::MethodNotSupported {
				exchange: self.name(),
				instrument: symbol.instrument,
			})),
		}
	}

	async fn asset_balance(&self, asset: Asset, recv_window: Option<u16>, _instrument: Instrument) -> ExchangeResult<AssetBalance> {
		account::asset_balance(self, asset, recv_window).await
	}

	async fn balances(&self, recv_window: Option<u16>, _instrument: Instrument) -> ExchangeResult<Balances> {
		account::balances(self, recv_window).await
	}
}

crate::define_provider_timeframe!(BybitInterval, ["1", "3", "5", "15", "30", "60", "120", "240", "360", "720", "D", "W", "M"]);
crate::define_provider_timeframe!(BybitIntervalTime, ["5min", "15min", "30min", "1h", "4h", "1d"]);
