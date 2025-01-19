use eyre::Result;
use serde::Deserialize;
use v_exchanges_adapters::mexc::{MexcAuth, MexcHttpUrl, MexcOption};
use v_utils::trades::Asset;

use crate::{AssetBalance, Balances};

pub async fn asset_balance(client: &v_exchanges_adapters::Client, asset: Asset) -> Result<AssetBalance> {
	let endpoint = format!("/api/v1/private/account/asset/{}", asset);
	let r: AssetBalanceResponse = client
		.get_no_query(&endpoint, [MexcOption::HttpUrl(MexcHttpUrl::Futures), MexcOption::HttpAuth(MexcAuth::Sign)])
		.await
		.unwrap();

	Ok(r.data.into())
}

/// Accepts recvWindow provision
pub async fn balances(client: &v_exchanges_adapters::Client) -> Result<Balances> {
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

#[allow(unused)]
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssetBalanceResponse {
	pub code: i32,
	pub data: AssetBalanceInfo,
	pub success: bool,
}

#[allow(unused)]
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssetBalanceInfo {
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

impl From<AssetBalanceInfo> for AssetBalance {
	fn from(r: AssetBalanceInfo) -> Self {
		Self {
			#[allow(clippy::unnecessary_fallible_conversions)] //Q: do I ever want them?
			asset: r.currency.try_into().expect("Assume v_utils is able to handle all mexc pairs"),
			underlying: r.equity,
			usd: None,
		}
	}
}
