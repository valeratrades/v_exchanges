use jiff::Timestamp;
use serde::Deserialize;
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges_adapters::binance::{BinanceAuth, BinanceHttpUrl, BinanceOption};
use v_utils::trades::{Asset, Pair, Usd};

use crate::{
	ExchangeResult,
	core::{ApiKeyInfo, AssetBalance, Balances, PersonalInfo},
};

pub async fn personal_info(client: &v_exchanges_adapters::Client, recv_window: Option<std::time::Duration>) -> ExchangeResult<PersonalInfo> {
	assert!(client.is_authenticated::<BinanceOption>());

	let mut balance_options = vec![BinanceOption::HttpUrl(BinanceHttpUrl::Spot), BinanceOption::HttpAuth(BinanceAuth::Sign)];
	let mut api_options = vec![BinanceOption::HttpUrl(BinanceHttpUrl::Spot), BinanceOption::HttpAuth(BinanceAuth::Sign)];
	if let Some(rw) = recv_window {
		balance_options.push(BinanceOption::RecvWindow(rw));
		api_options.push(BinanceOption::RecvWindow(rw));
	}

	let (balance_result, api_result) = tokio::join!(
		client.get_no_query::<AccountResponse, _>("/api/v3/account", balance_options),
		client.get_no_query::<ApiRestrictionsResponse, _>("/sapi/v1/account/apiRestrictions", api_options),
	);
	let account = balance_result?;
	let api_response = api_result?;

	let prices = super::market::prices(client, None).await?;

	let mut asset_balances: Vec<AssetBalance> = Vec::new();
	for b in account.balances {
		let underlying = b.free + b.locked;
		if underlying == 0. {
			continue;
		}
		let asset: Asset = (&*b.asset).into();
		let usd = if asset == "USDT" {
			Some(Usd(underlying))
		} else {
			let usdt_pair = Pair::new(asset, "USDT".into());
			prices.get(&usdt_pair).map(|p| Usd(underlying * p))
		};
		asset_balances.push(AssetBalance { asset, underlying, usd });
	}
	let total = asset_balances.iter().fold(Usd(0.), |acc, b| acc + b.usd.unwrap_or(Usd(0.)));

	let expire_time = api_response.expire_time.map(|ms| Timestamp::from_millisecond(ms).expect("Binance expireTime is valid ms timestamp"));

	Ok(PersonalInfo {
		api: ApiKeyInfo { expire_time },
		balances: Balances::new(asset_balances, total),
	})
}

#[serde_as]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountResponse {
	balances: Vec<SpotBalance>,
}

#[serde_as]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpotBalance {
	asset: String,
	#[serde_as(as = "DisplayFromStr")]
	free: f64,
	#[serde_as(as = "DisplayFromStr")]
	locked: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiRestrictionsResponse {
	/// Millisecond timestamp; absent when no expiry is set
	expire_time: Option<i64>,
}
