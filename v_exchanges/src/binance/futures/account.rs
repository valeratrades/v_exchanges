use std::fmt::Display;

use color_eyre::eyre::Result;
use serde::Deserialize;
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges_adapters::binance::{BinanceAuth, BinanceHttpUrl, BinanceOption};
use v_utils::{macros::ScreamIt, trades::{Asset, Kline, Ohlc, Pair, Side, Timeframe}};

use crate::core::AssetBalance;

// balance {{{
pub async fn asset_balance(client: &v_exchanges_adapters::Client, asset: Asset) -> Result<AssetBalance> {
	let balances = balances(client).await?;
	let balance = balances.into_iter().find(|b| b.asset == asset).unwrap();
	Ok(balance)
}

/// Accepts recvWindow provision
pub async fn balances(client: &v_exchanges_adapters::Client) -> Result<Vec<AssetBalance>> {
	let r: Vec<AssetBalanceResponse> = client
		.get_no_query("/fapi/v3/balance", [
			BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM),
			BinanceOption::HttpAuth(BinanceAuth::Sign),
		])
		.await
		.unwrap();
	Ok(r.into_iter().map(|r| r.into()).collect())
}
#[serde_as]
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssetBalanceResponse {
	pub account_alias: String,
	pub asset: String,
	#[serde_as(as = "DisplayFromStr")]
	pub balance: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub cross_wallet_balance: f64,
	#[serde(rename = "crossUnPnl")]
	#[serde_as(as = "DisplayFromStr")]
	pub cross_unrealized_pnl: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub available_balance: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub max_withdraw_amount: f64,
	pub margin_available: bool,
	pub update_time: u64,
}
impl From<AssetBalanceResponse> for AssetBalance {
	fn from(r: AssetBalanceResponse) -> Self {
		Self {
			asset: r.asset.into(),
			balance: r.balance,
			timestamp: r.update_time as i64,
		}
	}
}
//,}}}

#[derive(ScreamIt)]
pub enum ContractType {
	Perpetual,
	CurrentMonth,
	NextMonth,
	CurrentQuarter,
	NextQuarter,
}

#[derive(ScreamIt)]
pub enum PositionSide {
	Both,
	Long,
	Short,
}

#[derive(ScreamIt)]
pub enum OrderType {
	Limit,
	Market,
	Stop,
	StopMarket,
	TakeProfit,
	TakeProfitMarket,
	TrailingStopMarket,
}

#[derive(ScreamIt)]
pub enum WorkingType {
	MarkPrice,
	ContractPrice,
}

#[derive(ScreamIt)]
pub enum TimeInForce {
	Gtc,
	Ioc,
	Fok,
	Gtx,
}

struct OrderRequest {
	pub symbol: String,
	pub side: Side,
	pub position_side: Option<PositionSide>,
	pub order_type: OrderType,
	pub time_in_force: Option<TimeInForce>,
	pub qty: Option<f64>,
	pub reduce_only: Option<bool>,
	pub price: Option<f64>,
	pub stop_price: Option<f64>,
	pub close_position: Option<bool>,
	pub activation_price: Option<f64>,
	pub callback_rate: Option<f64>,
	pub working_type: Option<WorkingType>,
	pub price_protect: Option<f64>,
}

pub struct IncomeRequest {
	pub symbol: Option<String>,
	pub income_type: Option<IncomeType>,
	pub start_time: Option<u64>,
	pub end_time: Option<u64>,
	pub limit: Option<u32>,
}

#[derive(ScreamIt)]
pub enum IncomeType {
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
