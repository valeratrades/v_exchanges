use jiff::Timestamp;
use serde::Deserialize;
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges_adapters::binance::{BinanceAuth, BinanceHttpUrl, BinanceOption};
use v_utils::trades::{Asset, Pair, Usd};

use crate::{
	ExchangeResult,
	core::{ApiKeyInfo, AssetBalance, Balances, KeyPermission, PersonalInfo},
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

	let mut asset_balances: Vec<AssetBalance> = Vec::default();
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

	let expire_time = api_response
		.expire_time
		.map(|ms| Timestamp::from_millisecond(ms).expect("Binance expireTime is valid ms timestamp"));

	Ok(PersonalInfo {
		api: ApiKeyInfo {
			expire_time,
			permissions: api_response.into(),
		},
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
	#[allow(unused)]
	create_time: i64,
	#[allow(unused)]
	ip_restrict: bool,
	enable_reading: bool,
	enable_futures: bool,
	enable_spot_and_margin_trading: bool,
	enable_withdrawals: bool,
	enable_internal_transfer: bool,
	/// Absent for keys that don't have margin lending permissions configured
	enable_margin_loan: Option<bool>,
	enable_vanilla_options: bool,
	permits_universal_transfer: bool,
	enable_portfolio_margin_trading: bool,
	enable_fix_api_trade: bool,
	enable_fix_read_only: bool,
	enable_margin: bool,
}
impl From<ApiRestrictionsResponse> for Vec<KeyPermission> {
	fn from(r: ApiRestrictionsResponse) -> Self {
		let mut out = Vec::new();
		if r.enable_reading {
			out.push(KeyPermission::Read);
		}
		if r.enable_futures {
			out.push(KeyPermission::Futures);
		}
		if r.enable_spot_and_margin_trading {
			out.push(KeyPermission::SpotTrade);
		}
		if r.enable_withdrawals {
			out.push(KeyPermission::Withdraw);
		}
		if r.enable_internal_transfer || r.permits_universal_transfer {
			out.push(KeyPermission::Transfer);
		}
		if r.enable_margin_loan.unwrap_or(false) || r.enable_margin {
			out.push(KeyPermission::Margin);
		}
		if r.enable_vanilla_options {
			out.push(KeyPermission::Options);
		}
		if r.enable_portfolio_margin_trading {
			out.push(KeyPermission::Other("PortfolioMarginTrading".to_owned()));
		}
		if r.enable_fix_api_trade {
			out.push(KeyPermission::Other("FixApiTrade".to_owned()));
		}
		if r.enable_fix_read_only {
			out.push(KeyPermission::Other("FixReadOnly".to_owned()));
		}
		out
	}
}
