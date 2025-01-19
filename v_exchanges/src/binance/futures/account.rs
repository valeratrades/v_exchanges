use std::collections::BTreeMap;

use eyre::{Result, eyre};
use serde::Deserialize;
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges_adapters::binance::{BinanceAuth, BinanceHttpUrl, BinanceOption};
use v_utils::{
	macros::ScreamIt,
	trades::{Asset, Pair, Side, Usd},
};

use crate::core::{AssetBalance, Balances};

// balance {{{
//DUP: difficult to escape duplicating half the [balances] method due to a) not requiring usd value b) binance not having individual asset balance endpoint
pub async fn asset_balance(client: &v_exchanges_adapters::Client, asset: Asset) -> Result<AssetBalance> {
	assert!(<adapters::Client as adapters::GetOptions<adapters::binance::BinanceOptions>>::is_authenticated(client));
	let r: Vec<AssetBalanceResponse> = client
		.get_no_query("/fapi/v3/balance", [
			BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM),
			BinanceOption::HttpAuth(BinanceAuth::Sign),
		])
		.await
		.unwrap();
	let vec_balance: Vec<AssetBalance> = r
		.into_iter()
		.map(|r| AssetBalance {
			asset: r.asset.into(),
			underlying: r.balance,
			usd: None,
		})
		.collect();
	let balance = vec_balance.into_iter().find(|b| b.asset == asset).ok_or_else(|| eyre!("No balance returned for {asset}"))?;
	Ok(balance)
}

pub async fn balances(client: &v_exchanges_adapters::Client, prices: &BTreeMap<Pair, f64>) -> Result<Balances> {
	assert!(<adapters::Client as adapters::GetOptions<adapters::binance::BinanceOptions>>::is_authenticated(client));
	let rs: Vec<AssetBalanceResponse> = client
		.get_no_query("/fapi/v3/balance", [
			BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM),
			BinanceOption::HttpAuth(BinanceAuth::Sign),
		])
		.await
		.unwrap();

	fn usd_value(underlying: f64, asset: Asset, prices: &BTreeMap<Pair, f64>) -> Result<Usd> {
		if underlying == 0. {
			return Ok(Usd(0.));
		}
		if asset == "USDT" {
			return Ok(Usd(underlying));
		}
		let usdt_pair = Pair::new(asset, "USDT".into());
		let usdt_price = prices.get(&usdt_pair).ok_or_else(|| eyre!("No usdt price found for {asset}, which has non-zero balance."))?;
		Ok((underlying * usdt_price).into())
	}

	let mut balances: Vec<AssetBalance> = Vec::with_capacity(rs.len());
	for r in rs {
		let asset = r.asset.into();
		let underlying = r.balance;
		balances.push(AssetBalance {
			asset,
			underlying,
			usd: Some(usd_value(underlying, asset, prices)?),
		});
	}

	let non_zero: Vec<AssetBalance> = balances.iter().filter(|b| b.underlying != 0.).cloned().collect();
	let total = non_zero.iter().fold(Usd(0.), |acc, b| {
		acc + {
			match b.usd {
				Some(b) => b,
				None => Usd(0.),
			}
		}
	});

	Ok(Balances::new(non_zero, total))
}
#[allow(unused)]
#[serde_as]
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AssetBalanceResponse {
	account_alias: String,
	pub asset: String,
	#[serde_as(as = "DisplayFromStr")]
	pub balance: f64,
	#[serde_as(as = "DisplayFromStr")]
	cross_wallet_balance: f64,
	#[serde(rename = "crossUnPnl")]
	#[serde_as(as = "DisplayFromStr")]
	cross_unrealized_pnl: f64,
	#[serde_as(as = "DisplayFromStr")]
	available_balance: f64,
	#[serde_as(as = "DisplayFromStr")]
	max_withdraw_amount: f64,
	margin_available: bool,
	update_time: u64,
}
//,}}}

#[derive(ScreamIt)]
enum ContractType {
	Perpetual,
	CurrentMonth,
	NextMonth,
	CurrentQuarter,
	NextQuarter,
}

#[derive(ScreamIt)]
enum PositionSide {
	Both,
	Long,
	Short,
}

#[derive(ScreamIt)]
enum OrderType {
	Limit,
	Market,
	Stop,
	StopMarket,
	TakeProfit,
	TakeProfitMarket,
	TrailingStopMarket,
}

#[derive(ScreamIt)]
enum WorkingType {
	MarkPrice,
	ContractPrice,
}

#[derive(ScreamIt)]
enum TimeInForce {
	Gtc,
	Ioc,
	Fok,
	Gtx,
}

struct OrderRequest {
	symbol: String,
	side: Side,
	position_side: Option<PositionSide>,
	order_type: OrderType,
	time_in_force: Option<TimeInForce>,
	qty: Option<f64>,
	reduce_only: Option<bool>,
	price: Option<f64>,
	stop_price: Option<f64>,
	close_position: Option<bool>,
	activation_price: Option<f64>,
	callback_rate: Option<f64>,
	working_type: Option<WorkingType>,
	price_protect: Option<f64>,
}

struct IncomeRequest {
	symbol: Option<String>,
	income_type: Option<IncomeType>,
	start_time: Option<u64>,
	end_time: Option<u64>,
	limit: Option<u32>,
}

#[derive(ScreamIt)]
enum IncomeType {
	Transfer,
	WelcomeBonus,
	RealizedPnl,
	FundingFee,
	Commission,
	InsuranceClear,
	ReferralKickback,
	CommissionRebate,
	ApiRebate,
	ContestReward,
	CrossCollateralTransfer,
	OptionsPremiumFee,
	OptionsSettleProfit,
	InternalTransfer,
	AutoExchange,
	DeliveredSettlement,
	CoinSwapDeposit,
	CoinSwapWithdraw,
	PositionLimitIncreaseFee,
}
