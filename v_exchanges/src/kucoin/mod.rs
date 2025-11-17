mod account;
mod market;

pub use adapters::kucoin::KucoinOption;
use secrecy::SecretString;
use v_exchanges_adapters::Client;
use v_utils::trades::Asset;

use crate::{
	Balances, ExchangeName, ExchangeResult, Instrument, Symbol,
	core::{AssetBalance, Exchange},
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

	fn set_recv_window(&mut self, _recv_window: u16) {
		// Kucoin doesn't use recv_window in the same way as Binance/Bybit
	}

	async fn price(&self, symbol: Symbol, recv_window: Option<u16>) -> ExchangeResult<f64> {
		match symbol.instrument {
			Instrument::Spot => market::price(self, symbol.pair, recv_window).await,
			_ => unimplemented!(),
		}
	}

	async fn asset_balance(&self, asset: Asset, _instrument: Instrument, recv_window: Option<u16>) -> ExchangeResult<AssetBalance> {
		account::asset_balance(self, asset, recv_window).await
	}

	async fn balances(&self, _instrument: Instrument, recv_window: Option<u16>) -> ExchangeResult<Balances> {
		account::balances(self, recv_window).await
	}
}
