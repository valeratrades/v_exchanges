
use color_eyre::eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges_adapters::bybit::{BybitHttpAuth, BybitOption};
use v_utils::{macros::ScreamIt, trades::Asset};

use crate::core::AssetBalance;

pub async fn asset_balance(client: &v_exchanges_adapters::Client, asset: Asset) -> Result<AssetBalance> {
	let balances = balances(client).await?;
	let balance = balances.into_iter().find(|b| b.asset == asset).unwrap();
	Ok(balance)
}

/// Should be calling https://bybit-exchange.github.io/docs/v5/asset/balance/all-balance, but with how I'm registered on bybit, my key doesn't have permissions for that (they require it to be able to `transfer` for some reason)
pub async fn balances(client: &v_exchanges_adapters::Client) -> Result<Vec<AssetBalance>> {
	let value: serde_json::Value = client
		.get("/v5/account/wallet-balance", &[("accountType", "UNIFIED")], [BybitOption::HttpAuth(BybitHttpAuth::V3AndAbove)])
		.await?;

	let account_response: AccountResponse = serde_json::from_value(value)?;
	assert_eq!(account_response.result.list.len(), 1);
	let account_info = account_response.result.list.first().unwrap();

	let mut balances = Vec::new();
	for r in &account_info.coin {
		balances.push(AssetBalance {
			asset: (&*r.coin).into(),
			balance: r.wallet_balance,
			timestamp: account_response.time,
		});
	}
	Ok(balances)
}

#[derive(Debug, Clone, ScreamIt, Copy)]
pub enum AccountType {
	Spot,
	Contract,
	Unified,
	Funding,
	Option,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountResponse {
	pub result: AccountResult,
	pub ret_code: i64,
	pub ret_ext_info: RetExtInfo,
	pub ret_msg: String,
	pub time: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccountResult {
	pub list: Vec<AccountInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
	#[serde(rename = "accountIMRate")]
	pub account_im_rate: Option<Value>,
	#[serde(rename = "accountLTV")]
	pub account_ltv: Option<Value>,
	#[serde(rename = "accountMMRate")]
	pub account_mm_rate: Option<Value>,
	pub account_type: AccountType,
	pub coin: Vec<CoinInfo>,
	pub total_available_balance: String,
	pub total_equity: String,
	pub total_initial_margin: String,
	pub total_maintenance_margin: String,
	pub total_margin_balance: String,
	#[serde(rename = "totalPerpUPL")]
	pub total_perp_upl: String,
	pub total_wallet_balance: String,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoinInfo {
	pub accrued_interest: String,
	/// deprecated
	__available_to_borrow: Option<Value>, //? can I start it with __, will serde understand?
	pub available_to_withdraw: String,
	pub bonus: String,
	pub borrow_amount: String,
	pub coin: String,
	pub collateral_switch: bool,
	pub cum_realised_pnl: String,
	pub equity: String,
	pub locked: String,
	pub margin_collateral: bool,
	pub spot_hedging_qty: String,
	#[serde(rename = "totalOrderIM")]
	pub total_order_im: String,
	#[serde(rename = "totalPositionIM")]
	pub total_position_im: String,
	#[serde(rename = "totalPositionMM")]
	pub total_position_mm: String,
	#[serde_as(as = "DisplayFromStr")]
	pub unrealised_pnl: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub usd_value: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub wallet_balance: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RetExtInfo {}
