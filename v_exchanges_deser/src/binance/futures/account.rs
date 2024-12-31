use color_eyre::eyre::Result;
use serde::Deserialize;
use v_exchanges_adapters::binance::{BinanceHttpUrl, BinanceOption};
use v_utils::trades::{Kline, Ohlc, Pair, Timeframe};

//TODO: make a Coin type
pub async fn futures_balance(client: &v_exchanges_adapters::Client, asset: &str) -> Result<f64> {
	#[derive(serde::Serialize)]
	pub struct BalanceParams {
		pub asset: String,
	}
	let mut params = BalanceParams { asset: asset.to_string() };

	let r: AccountResponse = client.get("/fapi/v2/balance", Some(&params), [BinanceOption::Default]).await.unwrap();
	let balance = r.balance;
	Ok(balance)
}

#[derive(Clone, Debug, Default, derive_new::new, Deserialize)]
struct AccountResponse {
	pub balance: f64,
}
