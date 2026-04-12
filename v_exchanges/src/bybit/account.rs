use adapters::Client;
use ahash::AHashMap;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::{DisplayFromStr, serde_as};
use tracing::warn;
use v_exchanges_adapters::bybit::{BybitHttpAuth, BybitOption};
use v_utils::{macros::ScreamIt, trades::Asset};

use crate::{
	ExchangeResult,
	core::{ApiKeyInfo, AssetBalance, Balances, KeyPermission, PersonalInfo},
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

// Earn {{{
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EarnPositionResponse {
	result: EarnPositionResult,
}
#[derive(Debug, Deserialize)]
struct EarnPositionResult {
	list: Vec<EarnPosition>,
}
#[serde_as]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EarnPosition {
	coin: String,
	#[serde_as(as = "DisplayFromStr")]
	amount: f64,
}
//,}}}

pub(super) async fn personal_info(client: &Client, recv_window: Option<std::time::Duration>) -> ExchangeResult<PersonalInfo> {
	assert!(client.is_authenticated::<BybitOption>());

	let auth_options = |recv_window: Option<std::time::Duration>| {
		let mut options = vec![BybitOption::HttpAuth(BybitHttpAuth::V3AndAbove)];
		if let Some(rw) = recv_window {
			options.push(BybitOption::RecvWindow(rw));
		}
		options
	};

	let (balances_result, api_result) = tokio::join!(
		balances_inner(client, recv_window),
		client.get_no_query::<QueryApiResponse, _>("/v5/user/query-api", auth_options(recv_window)),
	);
	let balances = balances_result?;
	let api_response = api_result?;

	let expire_time = match api_response.result.expired_at.as_str() {
		"" | "0" => None,
		s => Some(
			s.parse::<Timestamp>()
				.unwrap_or_else(|e| panic!("Bybit expiredAt={s:?} failed to parse as ISO 8601 timestamp: {e}")),
		),
	};

	Ok(PersonalInfo {
		api: ApiKeyInfo {
			expire_time,
			permissions: api_response.result.permissions.into(),
		},
		balances,
	})
}

/// Should be calling https://bybit-exchange.github.io/docs/v5/asset/balance/all-balance, but with how I'm registered on bybit, my key doesn't have permissions for that (they require it to be able to `transfer` for some reason)
async fn balances_inner(client: &Client, recv_window: Option<std::time::Duration>) -> ExchangeResult<Balances> {
	assert!(client.is_authenticated::<BybitOption>());

	let auth_options = |recv_window: Option<std::time::Duration>| {
		let mut options = vec![BybitOption::HttpAuth(BybitHttpAuth::V3AndAbove)];
		if let Some(rw) = recv_window {
			options.push(BybitOption::RecvWindow(rw));
		}
		options
	};

	let account_response: AccountResponse = client.get("/v5/account/wallet-balance", &[("accountType", "UNIFIED")], auth_options(recv_window)).await?;
	assert_eq!(account_response.result.list.len(), 1);
	let account_info = account_response.result.list.first().unwrap();

	// Build coin→usd_rate map from UNIFIED account data for converting earn positions
	let mut usd_rates: AHashMap<String, f64> = AHashMap::default();
	let mut vec_balance = Vec::default();
	for r in &account_info.coin {
		if r.wallet_balance > 0.0 {
			usd_rates.insert(r.coin.clone(), r.usd_value / r.wallet_balance);
		}
		vec_balance.push(AssetBalance {
			asset: (&*r.coin).into(),
			underlying: r.wallet_balance,
			usd: Some(r.usd_value.into()),
		});
	}

	let mut total_equity = account_info.total_equity;

	// Fetch Earn positions (FlexibleSaving and OnChain) and add to total
	for category in ["FlexibleSaving", "OnChain"] {
		let r: Result<EarnPositionResponse, _> = client.get("/v5/earn/position", &[("category", category)], auth_options(recv_window)).await;
		match r {
			Ok(earn_response) => {
				for pos in &earn_response.result.list {
					if pos.amount == 0.0 {
						continue;
					}
					let usd_rate = match usd_rates.get(&pos.coin) {
						Some(&rate) => rate,
						None => {
							// Stablecoins pegged to USD
							match pos.coin.as_str() {
								"USDT" | "USDC" | "DAI" | "BUSD" => 1.0,
								_ => {
									warn!("No USD rate for earn coin {}, skipping", pos.coin);
									continue;
								}
							}
						}
					};
					let usd_value = pos.amount * usd_rate;
					total_equity += usd_value;

					// Merge into existing balance or add new entry
					if let Some(existing) = vec_balance.iter_mut().find(|b| {
						let asset: Asset = (&*pos.coin).into();
						b.asset == asset
					}) {
						existing.underlying += pos.amount;
						if let Some(ref mut usd) = existing.usd {
							*usd = v_utils::trades::Usd(**usd + usd_value);
						}
					} else {
						vec_balance.push(AssetBalance {
							asset: (&*pos.coin).into(),
							underlying: pos.amount,
							usd: Some(v_utils::trades::Usd(usd_value)),
						});
					}
				}
			}
			Err(e) => {
				warn!("Failed to fetch {category} earn positions: {e}");
			}
		}
	}

	let balances = Balances::new(vec_balance, total_equity.into());
	Ok(balances)
}

#[serde_as]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryApiResponse {
	result: QueryApiResult,
}
#[serde_as]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryApiResult {
	/// Expiry as ISO 8601 datetime string (e.g. `"2023-12-22T07:20:25Z"`); empty string or "0" means no expiry.
	expired_at: String,
	permissions: BybitPermissions,
}
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct BybitPermissions {
	contract_trade: Vec<String>,
	spot: Vec<String>,
	wallet: Vec<String>,
	options: Vec<String>,
	derivatives: Vec<String>,
	exchange: Vec<String>,
	earn: Vec<String>,
}
impl From<BybitPermissions> for Vec<KeyPermission> {
	fn from(p: BybitPermissions) -> Self {
		let mut out = Vec::new();
		// ContractTrade: "Order", "Position" → Futures
		if p.contract_trade.iter().any(|s| s == "Order" || s == "Position") {
			out.push(KeyPermission::Futures);
		}
		// Spot: "SpotTrade" → SpotTrade
		if p.spot.iter().any(|s| s == "SpotTrade") {
			out.push(KeyPermission::SpotTrade);
		}
		// Wallet: "AccountTransfer", "SubMemberTransfer" → Transfer; "Withdraw" → Withdraw
		if p.wallet.iter().any(|s| s == "AccountTransfer" || s == "SubMemberTransfer") {
			out.push(KeyPermission::Transfer);
		}
		if p.wallet.iter().any(|s| s == "Withdraw") {
			out.push(KeyPermission::Withdraw);
		}
		// Options: "OptionsTrade" → Options
		if p.options.iter().any(|s| s == "OptionsTrade") {
			out.push(KeyPermission::Options);
		}
		// Derivatives: "DerivativesTrade" → also Futures (if not already)
		if p.derivatives.iter().any(|s| s == "DerivativesTrade") && !out.contains(&KeyPermission::Futures) {
			out.push(KeyPermission::Futures);
		}
		// Exchange: "ExchangeHistory" → Other
		for s in p.exchange {
			out.push(KeyPermission::Other(format!("ExchangeHistory:{s}")));
		}
		// Earn: "Earn" → Earn
		if p.earn.iter().any(|s| s == "Earn") {
			out.push(KeyPermission::Earn);
		}
		out
	}
}
