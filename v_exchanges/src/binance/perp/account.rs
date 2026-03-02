use std::collections::BTreeMap;

use eyre::{Result, eyre};
use serde::Deserialize;
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges_adapters::binance::{BinanceAuth, BinanceHttpUrl, BinanceOption};
use v_utils::{
	macros::ScreamIt,
	trades::{Asset, Pair, Side, Usd},
};

use crate::{
	ExchangeResult,
	core::{AssetBalance, Balances},
};

// balance {{{
//DUP: difficult to escape duplicating half the [balances] method due to a) not requiring usd value b) binance not having individual asset balance endpoint
/// Place a new order on Binance Futures
pub async fn place_order(client: &v_exchanges_adapters::Client, request: OrderRequest, recv_window: Option<std::time::Duration>) -> ExchangeResult<OrderResponse> {
	assert!(client.is_authenticated::<BinanceOption>());

	let mut options = vec![BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM), BinanceOption::HttpAuth(BinanceAuth::Sign)];
	if let Some(rw) = recv_window {
		options.push(BinanceOption::RecvWindow(rw));
	}

	let mut params = vec![("symbol", request.symbol.clone()), ("side", request.side.to_string()), ("type", request.order_type.to_string())];

	if let Some(ps) = &request.position_side {
		params.push(("positionSide", ps.to_string()));
	}
	if let Some(tif) = &request.time_in_force {
		params.push(("timeInForce", tif.to_string()));
	}
	if let Some(qty) = request.qty {
		params.push(("quantity", qty.to_string()));
	}
	if let Some(price) = request.price {
		params.push(("price", price.to_string()));
	}
	if let Some(stop_price) = request.stop_price {
		params.push(("stopPrice", stop_price.to_string()));
	}
	if let Some(reduce_only) = request.reduce_only {
		params.push(("reduceOnly", reduce_only.to_string().to_uppercase()));
	}
	if let Some(close_pos) = request.close_position {
		params.push(("closePosition", close_pos.to_string().to_uppercase()));
	}
	if let Some(act_price) = request.activation_price {
		params.push(("activationPrice", act_price.to_string()));
	}
	if let Some(callback) = request.callback_rate {
		params.push(("callbackRate", callback.to_string()));
	}
	if let Some(wt) = &request.working_type {
		params.push(("workingType", wt.to_string()));
	}
	if let Some(pp) = request.price_protect {
		params.push(("priceProtect", pp.to_string().to_uppercase()));
	}
	if let Some(client_order_id) = &request.new_client_order_id {
		params.push(("newClientOrderId", client_order_id.clone()));
	}

	let response: OrderResponse = client.post("/fapi/v1/order", &params, options).await?;
	Ok(response)
}
/// Query income history
pub async fn income_history(client: &v_exchanges_adapters::Client, request: IncomeRequest, recv_window: Option<std::time::Duration>) -> ExchangeResult<Vec<IncomeRecord>> {
	assert!(client.is_authenticated::<BinanceOption>());

	let mut options = vec![BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM), BinanceOption::HttpAuth(BinanceAuth::Sign)];
	if let Some(rw) = recv_window {
		options.push(BinanceOption::RecvWindow(rw));
	}

	let mut params = vec![];

	if let Some(symbol) = &request.symbol {
		params.push(("symbol", symbol.clone()));
	}
	if let Some(income_type) = &request.income_type {
		params.push(("incomeType", income_type.to_string()));
	}
	if let Some(start_time) = request.start_time {
		params.push(("startTime", start_time.to_string()));
	}
	if let Some(end_time) = request.end_time {
		params.push(("endTime", end_time.to_string()));
	}
	if let Some(limit) = request.limit {
		params.push(("limit", limit.to_string()));
	}
	if let Some(page) = request.page {
		params.push(("page", page.to_string()));
	}

	let response: Vec<IncomeRecord> = client.get("/fapi/v1/income", &params, options).await?;
	Ok(response)
}
#[serde_as]
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderResponse {
	pub client_order_id: String,
	pub cum_qty: String,
	pub cum_quote: String,
	#[serde_as(as = "DisplayFromStr")]
	pub executed_qty: f64,
	pub order_id: u64,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub avg_price: Option<f64>,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub orig_qty: Option<f64>,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub price: Option<f64>,
	pub reduce_only: bool,
	pub side: String,
	pub position_side: String,
	pub status: String,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub stop_price: Option<f64>,
	pub close_position: bool,
	pub symbol: String,
	pub time_in_force: String,
	#[serde(rename = "type")]
	pub order_type: String,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub activation_price: Option<f64>,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub price_rate: Option<f64>,
	pub update_time: u64,
	pub working_type: String,
	pub price_protect: bool,
	#[serde(rename = "priceMatch")]
	pub price_match: Option<String>,
	pub self_trade_prevention_mode: Option<String>,
	pub good_till_date: Option<u64>,
}
#[serde_as]
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomeRecord {
	pub symbol: String,
	pub income_type: String,
	#[serde_as(as = "DisplayFromStr")]
	pub income: f64,
	pub asset: String,
	pub info: String,
	pub time: u64,
	pub tran_id: String,
	pub trade_id: String,
}
#[derive(Clone, Debug)]
pub struct OrderRequest {
	pub symbol: String,
	pub side: Side,
	pub order_type: OrderType,
	pub position_side: Option<PositionSide>,
	pub time_in_force: Option<TimeInForce>,
	pub qty: Option<f64>,
	pub price: Option<f64>,
	pub stop_price: Option<f64>,
	pub reduce_only: Option<bool>,
	pub close_position: Option<bool>,
	pub activation_price: Option<f64>,
	pub callback_rate: Option<f64>,
	pub working_type: Option<WorkingType>,
	pub price_protect: Option<bool>,
	pub new_client_order_id: Option<String>,
}
#[derive(Clone, Debug)]
pub struct IncomeRequest {
	pub symbol: Option<String>,
	pub income_type: Option<IncomeType>,
	pub start_time: Option<u64>,
	pub end_time: Option<u64>,
	pub limit: Option<u32>,
	pub page: Option<u32>,
}
#[derive(Clone, Debug, ScreamIt)]
pub enum ContractType {
	Perpetual,
	CurrentMonth,
	NextMonth,
	CurrentQuarter,
	NextQuarter,
}
#[derive(Clone, Debug, ScreamIt)]
pub enum PositionSide {
	Both,
	Long,
	Short,
}
#[derive(Clone, Debug, ScreamIt)]
pub enum OrderType {
	Limit,
	Market,
	Stop,
	StopMarket,
	TakeProfit,
	TakeProfitMarket,
	TrailingStopMarket,
}
#[derive(Clone, Debug, ScreamIt)]
pub enum WorkingType {
	MarkPrice,
	ContractPrice,
}
#[derive(Clone, Debug, ScreamIt)]
pub enum TimeInForce {
	Gtc,
	Ioc,
	Fok,
	Gtx,
}
#[derive(Clone, Debug, ScreamIt)]
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
pub(in crate::binance) async fn asset_balance(client: &v_exchanges_adapters::Client, asset: Asset, recv_window: Option<std::time::Duration>) -> ExchangeResult<AssetBalance> {
	assert!(client.is_authenticated::<BinanceOption>());
	let mut options = vec![BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM), BinanceOption::HttpAuth(BinanceAuth::Sign)];
	if let Some(rw) = recv_window {
		options.push(BinanceOption::RecvWindow(rw));
	}
	let r: Vec<AssetBalanceResponse> = client.get_no_query("/fapi/v3/balance", options).await?;
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

pub(in crate::binance) async fn balances(client: &v_exchanges_adapters::Client, recv_window: Option<std::time::Duration>, prices: &BTreeMap<Pair, f64>) -> ExchangeResult<Balances> {
	assert!(client.is_authenticated::<BinanceOption>());
	let mut options = vec![BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM), BinanceOption::HttpAuth(BinanceAuth::Sign)];
	if let Some(rw) = recv_window {
		options.push(BinanceOption::RecvWindow(rw));
	}
	let rs: Vec<AssetBalanceResponse> = client.get_no_query("/fapi/v3/balance", options).await?;

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

// Order Placement {{{

//,}}}

// Income History {{{

//,}}}

#[allow(unused)]
#[serde_as]
#[derive(Clone, Debug, Deserialize)]
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

// Response Types {{{

//,}}}

// Request/Enum Types {{{

//,}}}
