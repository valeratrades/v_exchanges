use adapters::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::{DisplayFromStr, serde_as};
use tracing::warn;
use v_exchanges_adapters::bybit::{BybitHttpAuth, BybitOption};
use v_utils::{macros::ScreamIt, trades::Asset};

use crate::{
	ExchangeResult,
	core::{AssetBalance, Balances},
};

#[derive(Clone, Copy, Debug, ScreamIt)]
pub enum AccountType {
	Spot,
	Contract,
	Unified,
	Funding,
	Option,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountResponse {
	pub result: AccountResult,
	pub ret_code: i64,
	pub ret_ext_info: RetExtInfo,
	pub ret_msg: String,
	pub time: i64,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct AccountResult {
	pub list: Vec<AccountInfo>,
}
#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
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
	pub total_available_balance: Option<Value>, // can be "" in portfolio-margin mode
	#[serde_as(as = "DisplayFromStr")]
	/// in USD
	pub total_equity: f64,
	pub total_initial_margin: Option<Value>,     // can be "" in portfolio-margin mode
	pub total_maintenance_margin: Option<Value>, // can be "" in portfolio-margin mode
	pub total_margin_balance: Option<Value>,     // can be "" in portfolio-margin mode
	#[serde(rename = "totalPerpUPL")]
	#[serde_as(as = "DisplayFromStr")]
	pub total_perp_upl: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub total_wallet_balance: f64,
}
#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoinInfo {
	#[serde_as(as = "DisplayFromStr")]
	pub accrued_interest: String,
	/// deprecated
	__available_to_borrow: Option<Value>,
	pub available_to_withdraw: Option<Value>, // deprecated for AccountType::UNIFIED, should use [Get Transerable Amount](<https://bybit-exchange.github.io/docs/v5/account/unified-trans-amnt>) for it instead
	pub bonus: Option<String>,                // specific to `UNIFIED` account type
	#[serde_as(as = "DisplayFromStr")]
	pub borrow_amount: f64,
	pub coin: String,
	free: Option<String>, // unique field for Classic `SPOT`
	pub collateral_switch: bool,
	#[serde_as(as = "DisplayFromStr")]
	pub cum_realised_pnl: f64,
	#[serde_as(as = "DisplayFromStr")]
	/// in base currency
	pub equity: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub locked: f64,
	pub margin_collateral: bool,
	#[serde_as(as = "DisplayFromStr")]
	pub spot_hedging_qty: f64,
	#[serde(rename = "totalOrderIM")]
	pub total_order_im: Option<Value>, // "" for portfolio-margin mode
	#[serde(rename = "totalPositionIM")]
	#[serde_as(as = "DisplayFromStr")]
	pub total_position_im: String,
	#[serde(rename = "totalPositionMM")]
	#[serde_as(as = "DisplayFromStr")]
	pub total_position_mm: String,
	#[serde_as(as = "DisplayFromStr")]
	pub unrealised_pnl: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub usd_value: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub wallet_balance: f64,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct RetExtInfo {}
pub(super) async fn asset_balance(client: &v_exchanges_adapters::Client, asset: Asset, recv_window: Option<std::time::Duration>) -> ExchangeResult<AssetBalance> {
	assert!(client.is_authenticated::<BybitOption>());
	let balances: Balances = balances(client, recv_window).await?;
	let balance: AssetBalance = balances.iter().find(|b| b.asset == asset).copied().unwrap_or_else(|| {
		warn!("No balance found for asset: {asset:?}");
		AssetBalance { asset, ..Default::default() }
	});
	Ok(balance)
}

/// Should be calling https://bybit-exchange.github.io/docs/v5/asset/balance/all-balance, but with how I'm registered on bybit, my key doesn't have permissions for that (they require it to be able to `transfer` for some reason)
pub(super) async fn balances(client: &Client, recv_window: Option<std::time::Duration>) -> ExchangeResult<Balances> {
	assert!(client.is_authenticated::<BybitOption>());

	let mut options = vec![BybitOption::HttpAuth(BybitHttpAuth::V3AndAbove)];
	if let Some(rw) = recv_window {
		options.push(BybitOption::RecvWindow(rw));
	}
	let account_response: AccountResponse = client.get("/v5/account/wallet-balance", &[("accountType", "UNIFIED")], options).await?;
	assert_eq!(account_response.result.list.len(), 1);
	let account_info = account_response.result.list.first().unwrap();

	let mut vec_balance = Vec::new();
	for r in &account_info.coin {
		vec_balance.push(AssetBalance {
			asset: (&*r.coin).into(),
			underlying: r.wallet_balance,
			usd: Some(r.usd_value.into()),
		});
	}
	let balances = Balances::new(vec_balance, account_info.total_equity.into());
	Ok(balances)
}
