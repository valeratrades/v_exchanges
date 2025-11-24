mod account;
mod market;

pub use adapters::kucoin::KucoinOption;

crate::define_provider_timeframe!(KucoinTimeframe, ["1m", "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h", "8h", "12h", "1d", "1w"]);
use std::collections::BTreeMap;

use secrecy::SecretString;
use v_exchanges_adapters::Client;
use v_utils::trades::{Asset, Pair, Timeframe};

use crate::{
	Balances, ExchangeName, ExchangeResult, Instrument, RequestRange, Symbol,
	core::{AssetBalance, Exchange, ExchangeInfo, Klines},
};

#[derive(Clone, Debug, Default, derive_more::Deref, derive_more::DerefMut)]
pub struct Kucoin(pub Client);

#[async_trait::async_trait]
impl Exchange for Kucoin {
	fn name(&self) -> ExchangeName {
		ExchangeName::Kucoin
	}

	fn auth(&mut self, pubkey: String, secret: SecretString) {
		self.update_default_option(KucoinOption::Pubkey(pubkey));
		self.update_default_option(KucoinOption::Secret(secret));
		// Note: Passphrase needs to be set separately via KucoinOption::Passphrase
	}

	fn set_recv_window(&mut self, _recv_window: std::time::Duration) {
		tracing::warn!("KuCoin does not support configurable recv_window - uses a fixed 5-second tolerance window for all authenticated requests");
	}

	async fn exchange_info(&self, instrument: Instrument) -> ExchangeResult<ExchangeInfo> {
		match instrument {
			Instrument::Spot => market::exchange_info(self, None).await,
			_ => unimplemented!(),
		}
	}

	async fn price(&self, symbol: Symbol) -> ExchangeResult<f64> {
		match symbol.instrument {
			Instrument::Spot => market::price(self, symbol.pair, None).await,
			_ => unimplemented!(),
		}
	}

	async fn prices(&self, pairs: Option<Vec<Pair>>, instrument: Instrument) -> ExchangeResult<BTreeMap<Pair, f64>> {
		match instrument {
			Instrument::Spot => market::prices(self, pairs, None).await,
			_ => unimplemented!(),
		}
	}

	async fn klines(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Klines> {
		match symbol.instrument {
			Instrument::Spot => market::klines(self, symbol, tf.try_into()?, range, None).await,
			_ => unimplemented!(),
		}
	}

	async fn asset_balance(&self, asset: Asset, _instrument: Instrument, recv_window: Option<std::time::Duration>) -> ExchangeResult<AssetBalance> {
		account::asset_balance(self, asset, recv_window).await
	}

	async fn balances(&self, _instrument: Instrument, recv_window: Option<std::time::Duration>) -> ExchangeResult<Balances> {
		account::balances(self, recv_window).await
	}
}
