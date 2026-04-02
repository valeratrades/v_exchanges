mod account;
mod market;
mod ws;

use adapters::bybit::{BybitOption, BybitOptions};
use secrecy::SecretString;
use v_exchanges_adapters::{Client, GetOptions};
use v_utils::trades::{Pair, Timeframe};

use crate::{
	BookUpdate, ExchangeError, ExchangeInfo, ExchangeName, ExchangeResult, ExchangeStream, Instrument, MethodError, OpenInterest, Symbol,
	core::{ExchangeImpl, Klines, PersonalInfo, RequestRange},
};

#[derive(Clone, Debug, Default, derive_more::Deref, derive_more::DerefMut)]
pub struct Bybit(pub Client);

//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
#[async_trait::async_trait]
impl ExchangeImpl for Bybit {
	fn name(&self) -> ExchangeName {
		ExchangeName::Bybit
	}

	fn auth(&mut self, pubkey: String, secret: SecretString) {
		self.update_default_option(BybitOption::Pubkey(pubkey));
		self.update_default_option(BybitOption::Secret(secret));
	}

	fn set_recv_window(&mut self, recv_window: std::time::Duration) {
		self.update_default_option(BybitOption::RecvWindow(recv_window));
	}

	fn default_recv_window(&self) -> Option<std::time::Duration> {
		GetOptions::<BybitOptions>::default_options(&**self).recv_window
	}

	async fn exchange_info(&self, instrument: Instrument) -> ExchangeResult<ExchangeInfo> {
		match instrument {
			Instrument::Perp | Instrument::PerpInverse => market::exchange_info(self, instrument).await,
			_ => unimplemented!(),
		}
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

	async fn personal_info(&self, _instrument: Instrument, recv_window: Option<std::time::Duration>) -> ExchangeResult<PersonalInfo> {
		account::personal_info(self, recv_window).await
	}

	fn ws_book(&self, pairs: Vec<Pair>, instrument: Instrument) -> Result<Box<dyn ExchangeStream<Item = BookUpdate>>, ExchangeError> {
		match instrument {
			Instrument::Perp | Instrument::Spot => {
				let connection = ws::BookConnection::new(self, pairs, instrument)?;
				Ok(Box::new(connection))
			}
			_ => Err(ExchangeError::Method(MethodError::MethodNotImplemented { exchange: self.name(), instrument })),
		}
	}
}

crate::define_provider_timeframe!(BybitInterval, ["1", "3", "5", "15", "30", "60", "120", "240", "360", "720", "D", "W", "M"]);
crate::define_provider_timeframe!(BybitIntervalTime, ["5min", "15min", "30min", "1h", "4h", "1d"]);
