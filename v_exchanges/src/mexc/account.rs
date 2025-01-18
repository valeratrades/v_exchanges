use eyre::Result;
use serde::Deserialize;
use v_exchanges_adapters::mexc::{MexcAuth, MexcHttpUrl, MexcOption};
use v_utils::trades::Asset;

use crate::core::AssetBalance;

pub async fn asset_balance(client: &v_exchanges_adapters::Client, asset: Asset) -> Result<AssetBalance> {
	let endpoint = format!("/api/v1/private/account/asset/{}", asset);
	let r: AssetBalanceFullResponse = client
		.get_no_query(&endpoint, [MexcOption::HttpUrl(MexcHttpUrl::Futures), MexcOption::HttpAuth(MexcAuth::Sign)])
		.await
		.unwrap();

	Ok(r.data.into())
}

/// Accepts recvWindow provision
pub async fn balances(client: &v_exchanges_adapters::Client) -> Result<Vec<AssetBalance>> {
	//TODO!: \
	//assert!(client.is_authenticated());
	todo!();
	//let r: Vec<AssetBalanceResponse> = client
	//	.get_no_query("/fapi/v3/balance", [
	//		BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM),
	//		BinanceOption::HttpAuth(BinanceAuth::Sign),
	//	])
	//	.await
	//	.unwrap();
	//Ok(r.into_iter().map(|r| r.into()).collect())
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssetBalanceFullResponse {
	pub code: i32,
	pub data: AssetBalanceResponse,
	pub success: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssetBalanceResponse {
	pub available_balance: f64,
	pub available_cash: f64,
	pub available_open: f64,
	pub bonus: f64,
	pub cash_balance: f64,
	pub currency: String,
	pub equity: f64,
	pub frozen_balance: f64,
	pub position_margin: f64,
	pub unrealized: f64,
}

impl From<AssetBalanceResponse> for AssetBalance {
	fn from(r: AssetBalanceResponse) -> Self {
		Self {
			asset: r.currency.try_into().expect("Assume v_utils is able to handle all mexc pairs"),
			balance: r.equity,
		}
	}
}
