use std::fmt::Display;

use color_eyre::eyre::Result;
use serde::Deserialize;
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges_adapters::binance::{BinanceAuth, BinanceHttpUrl, BinanceOption};
use v_utils::trades::{Asset, Kline, Ohlc, Pair, Side, Timeframe};

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
			timestamp: r.update_time,
		}
	}
}
//,}}}

pub enum ContractType {
	Perpetual,
	CurrentMonth,
	NextMonth,
	CurrentQuarter,
	NextQuarter,
}
impl Display for ContractType {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			ContractType::Perpetual => write!(f, "PERPETUAL"),
			ContractType::CurrentMonth => write!(f, "CURRENT_MONTH"),
			ContractType::NextMonth => write!(f, "NEXT_MONTH"),
			ContractType::CurrentQuarter => write!(f, "CURRENT_QUARTER"),
			ContractType::NextQuarter => write!(f, "NEXT_QUARTER"),
		}
	}
}

pub enum PositionSide {
	Both,
	Long,
	Short,
}
impl Display for PositionSide {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Both => write!(f, "BOTH"),
			Self::Long => write!(f, "LONG"),
			Self::Short => write!(f, "SHORT"),
		}
	}
}

pub enum OrderType {
	Limit,
	Market,
	Stop,
	StopMarket,
	TakeProfit,
	TakeProfitMarket,
	TrailingStopMarket,
}
impl Display for OrderType {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Limit => write!(f, "LIMIT"),
			Self::Market => write!(f, "MARKET"),
			Self::Stop => write!(f, "STOP"),
			Self::StopMarket => write!(f, "STOP_MARKET"),
			Self::TakeProfit => write!(f, "TAKE_PROFIT"),
			Self::TakeProfitMarket => write!(f, "TAKE_PROFIT_MARKET"),
			Self::TrailingStopMarket => write!(f, "TRAILING_STOP_MARKET"),
		}
	}
}

pub enum WorkingType {
	MarkPrice,
	ContractPrice,
}
impl Display for WorkingType {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::MarkPrice => write!(f, "MARK_PRICE"),
			Self::ContractPrice => write!(f, "CONTRACT_PRICE"),
		}
	}
}

#[allow(clippy::all)]
pub enum TimeInForce {
	Gtc,
	Ioc,
	Fok,
	Gtx,
}
impl Display for TimeInForce {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Gtc => write!(f, "GTC"),
			Self::Ioc => write!(f, "IOC"),
			Self::Fok => write!(f, "FOK"),
			Self::Gtx => write!(f, "GTX"),
		}
	}
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

#[allow(non_camel_case_types)]
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
//TODO!: figure out how to automatically derive this. Probably with derive_more::derive::Display and serde(rename = "UPPERCASE"). Or better yet just fork derive_more::Display
impl Display for IncomeType {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Transfer => write!(f, "TRANSFER"),
			Self::WelcomeBonus => write!(f, "WELCOME_BONUS"),
			Self::RealizedPnl => write!(f, "REALIZED_PNL"),
			Self::FundingFee => write!(f, "FUNDING_FEE"),
			Self::Commission => write!(f, "COMMISSION"),
			Self::InsuranceClear => write!(f, "INSURANCE_CLEAR"),
			Self::ReferralKickback => write!(f, "REFERRAL_KICKBACK"),
			Self::CommissionRebate => write!(f, "COMMISSION_REBATE"),
			Self::ApiRebate => write!(f, "API_REBATE"),
			Self::ContestReward => write!(f, "CONTEST_REWARD"),
			Self::CrossCollateralTransfer => write!(f, "CROSS_COLLATERAL_TRANSFER"),
			Self::OptionsPremiumFee => write!(f, "OPTIONS_PREMIUM_FEE"),
			Self::OptionsSettleProfit => write!(f, "OPTIONS_SETTLE_PROFIT"),
			Self::InternalTransfer => write!(f, "INTERNAL_TRANSFER"),
			Self::AutoExchange => write!(f, "AUTO_EXCHANGE"),
			Self::DeliveredSettlement => write!(f, "DELIVERED_SETTELMENT"),
			Self::CoinSwapDeposit => write!(f, "COIN_SWAP_DEPOSIT"),
			Self::CoinSwapWithdraw => write!(f, "COIN_SWAP_WITHDRAW"),
			Self::PositionLimitIncreaseFee => write!(f, "POSITION_LIMIT_INCREASE_FEE"),
		}
	}
}
