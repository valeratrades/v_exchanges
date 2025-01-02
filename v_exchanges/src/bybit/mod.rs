pub mod account;

use color_eyre::eyre::Result;
use v_exchanges_adapters::Client;
use v_exchanges_adapters::bybit;
use v_utils::trades::{Asset, Pair, Timeframe};

use crate::core::{AssetBalance, Exchange, Klines};

pub struct Bybit(pub Client);
impl Bybit {
	pub fn new() -> Self {
		Self(Client::new())
	}
}
impl std::ops::Deref for Bybit {
	type Target = Client;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}
impl std::ops::DerefMut for Bybit {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}


//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
impl Exchange for Bybit {
	async fn futures_klines(&self, symbol: Pair, tf: Timeframe, limit: u32, start_time: Option<u64>, end_time: Option<u64>) -> Result<Klines> {
		//futures::market::klines(&self.0, symbol, tf, limit, start_time, end_time).await
		todo!();
	}

	async fn futures_price(&self, symbol: Pair) -> Result<f64> {
		//futures::market::price(&self.0, symbol).await
		todo!();
	}

	async fn futures_asset_balance(&self, asset: Asset) -> Result<AssetBalance> {
		//futures::account::asset_balance(&self.0, asset).await
		todo!();
	}

	async fn futures_balances(&self) -> Result<Vec<AssetBalance>> {
		account::balances(&self.0).await
	}

	//DO: async fn balance(&self,
}
