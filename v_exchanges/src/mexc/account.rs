use adapters::{
	Client,
	mexc::{MexcAuth, MexcHttpUrl, MexcOption, MexcOptions},
};
use v_utils::prelude::*;

use crate::{AssetBalance, Balances, ExchangeResult, recv_window_check};

pub async fn asset_balance(client: &Client, asset: Asset, recv_window: Option<std::time::Duration>) -> ExchangeResult<AssetBalance> {
	use v_exchanges_adapters::GetOptions;
	recv_window_check!(recv_window, GetOptions::<MexcOptions>::default_options(client));
	assert!(client.is_authenticated::<MexcOption>());
	let mut options = vec![MexcOption::HttpUrl(MexcHttpUrl::Futures), MexcOption::HttpAuth(MexcAuth::Sign)];
	if let Some(rw) = recv_window {
		options.push(MexcOption::RecvWindow(rw));
	}
	let endpoint = format!("/api/v1/private/account/asset/{asset}");
	let r: AssetBalanceResponse = client.get_no_query(&endpoint, options).await.unwrap();

	Ok(r.data.into())
}

pub async fn balances(client: &Client, recv_window: Option<std::time::Duration>) -> ExchangeResult<Balances> {
	use v_exchanges_adapters::GetOptions;
	recv_window_check!(recv_window, GetOptions::<MexcOptions>::default_options(client));
	assert!(client.is_authenticated::<MexcOption>());
	let mut options = vec![MexcOption::HttpUrl(MexcHttpUrl::Futures), MexcOption::HttpAuth(MexcAuth::Sign)];
	if let Some(rw) = recv_window {
		options.push(MexcOption::RecvWindow(rw));
	}
	let rs: BalancesResponse = client.get_no_query("/api/v1/private/account/assets", options).await.unwrap();

	let non_zero: Vec<AssetBalance> = rs.data.into_iter().filter(|r| r.equity != 0.).map(|r| r.into()).collect();
	// dance with tambourine to request for usdt prices of all assets except usdt itself
	//RELIES: join_all preserving order
	let price_handles: Vec<_> = non_zero
		.iter()
		.map(|b| {
			if b.asset == "USDT" {
				Box::pin(async move { Ok(1.) }) as Pin<Box<dyn Future<Output = ExchangeResult<f64>> + Send>>
			} else {
				Box::pin(super::market::price(client, (b.asset, "USDT".into()).into(), recv_window)) as Pin<Box<dyn Future<Output = ExchangeResult<f64>> + Send>>
			}
		})
		.collect();
	let prices = join_all(price_handles).await.into_iter().collect::<ExchangeResult<Vec<f64>>>()?;

	let balances: Vec<AssetBalance> = non_zero
		.into_iter()
		.zip(prices.into_iter())
		.map(|(mut b, p)| {
			b.usd = Some((p * b.underlying).into());
			b
		})
		.collect();

	let total = balances.iter().fold(Usd(0.), |acc, b| acc + b.usd.expect("Just set for all"));
	Ok(Balances::new(balances, total))
}

#[allow(unused)]
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssetBalanceResponse {
	pub code: i32,
	pub data: AssetBalanceData,
	pub success: bool,
}
#[allow(unused)]
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssetBalanceData {
	available_balance: f64,
	available_cash: f64,
	available_open: f64,
	bonus: f64,
	cash_balance: f64,
	currency: String,
	equity: f64,
	frozen_balance: f64,
	position_margin: f64,
	unrealized: f64,
}
impl From<AssetBalanceData> for AssetBalance {
	fn from(r: AssetBalanceData) -> Self {
		Self {
			#[allow(clippy::unnecessary_fallible_conversions)] //Q: do I ever want them?
			asset: r.currency.try_into().expect("Assume v_utils is able to handle all mexc pairs"),
			underlying: r.equity,
			usd: None,
		}
	}
}

#[allow(unused)]
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BalancesResponse {
	pub code: i32,
	pub data: Vec<AssetBalanceData>,
	pub success: bool,
}
