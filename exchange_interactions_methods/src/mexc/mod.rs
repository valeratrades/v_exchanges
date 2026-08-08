mod account;
mod market;

use std::collections::BTreeMap;

use adapters::mexc::{MexcOption, MexcOptions};
use derive_more::derive::{Deref, DerefMut};
use exchange_interactions_adapters::{Client, GetOptions};
use secrecy::SecretString;
use trading_data_core::Pair;
use v_utils::Timeframe;

use crate::{
	ExchangeInfo, ExchangeName, ExchangeResult, Instrument, Symbol,
	core::{Account, ExchangeSeal, Klines, Market, PersonalInfo, RequestRange, validate_recv_window},
};

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct Mexc {
	#[deref]
	#[deref_mut]
	pub client: Client,
	pub info_cache: BTreeMap<Instrument, ExchangeInfo>,
}

impl ExchangeSeal for Mexc {}

#[async_trait::async_trait]
impl Account for Mexc {
	fn auth(&mut self, pubkey: String, secret: SecretString) {
		self.update_default_option(MexcOption::Pubkey(pubkey));
		self.update_default_option(MexcOption::Secret(secret));
	}

	fn set_recv_window(&mut self, recv_window: std::time::Duration) {
		self.update_default_option(MexcOption::RecvWindow(recv_window));
	}

	fn default_recv_window(&self) -> Option<std::time::Duration> {
		GetOptions::<MexcOptions>::default_options(&**self).recv_window
	}

	async fn personal_info(&self, instrument: Instrument, recv_window: Option<std::time::Duration>) -> ExchangeResult<PersonalInfo> {
		validate_recv_window(recv_window, self.default_recv_window())?;
		match instrument {
			Instrument::Perp => account::personal_info(self, recv_window).await,
			_ => unimplemented!(),
		}
	}
}

//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
#[async_trait::async_trait]
impl Market for Mexc {
	fn name(&self) -> ExchangeName {
		ExchangeName::Mexc
	}

	async fn prices(&self, _pairs: Option<Vec<Pair>>, instrument: Instrument) -> ExchangeResult<BTreeMap<Pair, f64>> {
		match instrument {
			Instrument::Perp => unimplemented!("Mexc does not have a multi-asset endpoints for futures"),
			_ => unimplemented!(),
		}
	}

	async fn klines(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Klines> {
		match symbol.instrument {
			Instrument::Perp => market::klines(self, symbol, tf.try_into()?, range).await,
			_ => unimplemented!(),
		}
	}
}

crate::define_provider_timeframe!(MexcTimeframe, ["1m", "5m", "15m", "30m", "60m", "4h", "1d", "1W", "1M"]);
