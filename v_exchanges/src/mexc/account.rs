use eyre::Result;
use serde::Deserialize;
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges_adapters::binance::{BinanceAuth, BinanceHttpUrl, BinanceOption};
use v_utils::{
	macros::ScreamIt,
	trades::{Asset, Side},
};

use crate::core::AssetBalance;

pub async fn asset_balance(client: &v_exchanges_adapters::Client, asset: Asset) -> Result<AssetBalance> {
	let endpoint = format!("/api/v1/private/account/asset/{}", asset);
	let r: serde_json::Value = client
		.get_no_query(&endpoint, [BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM), BinanceOption::HttpAuth(BinanceAuth::Sign)])
		.await
		.unwrap();
	let balances = balances(client).await?;
	let balance = balances.into_iter().find(|b| b.asset == asset).unwrap();
	Ok(balance)
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
