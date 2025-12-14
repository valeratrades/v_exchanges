use adapters::Client;
use eyre::{Result, eyre};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use tracing::warn;
use v_exchanges_adapters::kucoin::{KucoinAuth, KucoinHttpUrl, KucoinOption};
use v_utils::trades::{Asset, Pair, Usd};

use crate::{
	ExchangeResult,
	core::{AssetBalance, Balances},
	kucoin::market,
};

pub(super) async fn asset_balance(client: &v_exchanges_adapters::Client, asset: Asset, _recv_window: Option<std::time::Duration>) -> ExchangeResult<AssetBalance> {
	assert!(client.is_authenticated::<KucoinOption>());
	let balances: Balances = balances(client, None).await?;
	let balance: AssetBalance = balances.iter().find(|b| b.asset == asset).copied().unwrap_or_else(|| {
		warn!("No balance found for asset: {:?}", asset);
		AssetBalance { asset, ..Default::default() }
	});
	Ok(balance)
}

pub(super) async fn balances(client: &Client, recv_window: Option<std::time::Duration>) -> ExchangeResult<Balances> {
	assert!(client.is_authenticated::<KucoinOption>());

	let options = vec![KucoinOption::HttpAuth(KucoinAuth::Sign), KucoinOption::HttpUrl(KucoinHttpUrl::Spot)];
	let empty_params: &[(String, String)] = &[];
	let account_response: AccountResponse = client.get("/api/v1/accounts", empty_params, options).await?;

	// Helper function to calculate USD value for an asset
	async fn usd_value(client: &Client, underlying: f64, asset: Asset, recv_window: Option<std::time::Duration>) -> Result<Usd> {
		if underlying == 0. {
			return Ok(Usd(0.));
		}
		// Check common stablecoins
		if asset == "USDT" || asset == "USDC" || asset == "BUSD" || asset == "DAI" {
			return Ok(Usd(underlying));
		}
		// Fetch price for non-stablecoin assets
		let usdt_pair = Pair::new(asset, "USDT".into());
		let usdt_price = market::price(client, usdt_pair, None)
			.await
			.map_err(|e| eyre!("Failed to fetch USDT price for {asset} (balance: {underlying}): {e}"))?;
		Ok((underlying * usdt_price).into())
	}

	let mut balances: Vec<AssetBalance> = Vec::new();
	for account in &account_response.data {
		// Only include accounts with non-zero balances
		if account.balance > 0.0 {
			let asset: Asset = (&*account.currency).into();
			let underlying = account.balance;
			let usd = usd_value(client, underlying, asset, recv_window).await.ok();

			balances.push(AssetBalance { asset, underlying, usd });
		}
	}

	let total = balances.iter().fold(Usd(0.), |acc, b| {
		acc + match b.usd {
			Some(b) => b,
			None => Usd(0.),
		}
	});

	Ok(Balances::new(balances, total))
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
