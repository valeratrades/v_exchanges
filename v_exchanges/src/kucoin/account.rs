use adapters::Client;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use tracing::warn;
use v_exchanges_adapters::kucoin::{KucoinAuth, KucoinHttpUrl, KucoinOption};
use v_utils::trades::Asset;

use crate::{
	ExchangeResult,
	core::{AssetBalance, Balances},
};

pub async fn asset_balance(client: &v_exchanges_adapters::Client, asset: Asset, _recv_window: Option<u16>) -> ExchangeResult<AssetBalance> {
	assert!(client.is_authenticated::<KucoinOption>());
	let balances: Balances = balances(client, None).await?;
	let balance: AssetBalance = balances.iter().find(|b| b.asset == asset).copied().unwrap_or_else(|| {
		warn!("No balance found for asset: {:?}", asset);
		AssetBalance { asset, ..Default::default() }
	});
	Ok(balance)
}

pub async fn balances(client: &Client, _recv_window: Option<u16>) -> ExchangeResult<Balances> {
	assert!(client.is_authenticated::<KucoinOption>());

	let options = vec![KucoinOption::HttpAuth(KucoinAuth::Sign), KucoinOption::HttpUrl(KucoinHttpUrl::Spot)];
	let empty_params: &[(String, String)] = &[];
	let account_response: AccountResponse = client.get("/api/v1/accounts", empty_params, options).await?;

	let mut vec_balance = Vec::new();
	let total_usd = 0.0;

	for account in &account_response.data {
		// Only include accounts with non-zero balances
		if account.balance > 0.0 {
			vec_balance.push(AssetBalance {
				asset: (&*account.currency).into(),
				underlying: account.balance,
				usd: None, // Kucoin doesn't provide USD values in this endpoint
			});
		}
	}

	let balances = Balances::new(vec_balance, total_usd.into());
	Ok(balances)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountResponse {
	pub code: String,
	pub data: Vec<AccountData>,
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountData {
	pub id: String,
	pub currency: String,
	#[serde(rename = "type")]
	pub account_type: String,
	#[serde_as(as = "DisplayFromStr")]
	pub balance: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub available: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub holds: f64,
}
