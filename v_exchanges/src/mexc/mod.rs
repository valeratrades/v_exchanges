mod account;
mod market;

use std::collections::BTreeMap;

use adapters::mexc::MexcOption;
use derive_more::derive::{Deref, DerefMut};
use secrecy::SecretString;
use v_exchanges_adapters::Client;
use v_utils::trades::{Asset, Pair};

use crate::{
	Balances, ExchangeName, ExchangeResult, Instrument, Symbol,
	core::{AssetBalance, Exchange},
};

#[derive(Clone, Debug, Default, Deref, DerefMut)]
pub struct Mexc(pub Client);

//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
#[async_trait::async_trait]
impl Exchange for Mexc {
	fn name(&self) -> ExchangeName {
		ExchangeName::Mexc
	}

	fn auth(&mut self, pubkey: String, secret: SecretString) {
		self.update_default_option(MexcOption::Pubkey(pubkey));
		self.update_default_option(MexcOption::Secret(secret));
	}

	fn set_recv_window(&mut self, recv_window: u16) {
		self.update_default_option(MexcOption::RecvWindow(recv_window));
	}

	async fn prices(&self, _pairs: Option<Vec<Pair>>, instrument: Instrument) -> ExchangeResult<BTreeMap<Pair, f64>> {
		match instrument {
			Instrument::Perp => unimplemented!("Mexc does not have a multi-asset endpoints for futures"),
			_ => unimplemented!(),
		}
	}

	async fn price(&self, symbol: Symbol) -> ExchangeResult<f64> {
		match symbol.instrument {
			Instrument::Perp => market::price(self, symbol.pair).await,
			_ => unimplemented!(),
		}
	}

	async fn asset_balance(&self, asset: Asset, recv_window: Option<u16>, instrument: Instrument) -> ExchangeResult<AssetBalance> {
		match instrument {
			Instrument::Perp => account::asset_balance(self, asset, recv_window).await,
			_ => unimplemented!(),
		}
	}

	async fn balances(&self, recv_window: Option<u16>, instrument: Instrument) -> ExchangeResult<Balances> {
		match instrument {
			Instrument::Perp => account::balances(self, recv_window).await,
			_ => unimplemented!(),
		}
	}
}

crate::define_provider_timeframe!(MexcTimeframe, ["1m", "5m", "15m", "30m", "60m", "4h", "1d", "1W", "1M"]);
