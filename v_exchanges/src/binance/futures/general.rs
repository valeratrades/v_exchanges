
use adapters::binance::{BinanceHttpUrl, BinanceOption};
use chrono::DateTime;
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::{DisplayFromStr, serde_as};
use v_utils::trades::Pair;

use crate::core::{ExchangeInfo, PairInfo};
//TODO: general endpoints, like ping and exchange info

pub async fn exchange_info(client: &v_exchanges_adapters::Client) -> Result<ExchangeInfo> {
	let r: BinanceExchangeFutures = client.get_no_query("/fapi/v1/exchangeInfo", [BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM)]).await?;
	Ok(r.into())
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BinanceExchangeFutures {
	pub exchange_filters: Vec<String>,
	pub rate_limits: Vec<RateLimit>,
	pub server_time: i64,
	pub assets: Vec<Value>,
	pub symbols: Vec<FuturesSymbol>,
	pub timezone: String,
}

impl From<BinanceExchangeFutures> for ExchangeInfo {
	fn from(v: BinanceExchangeFutures) -> Self {
		Self {
			server_time: DateTime::from_timestamp_millis(v.server_time).unwrap(),
			pairs: v
				.symbols
				.into_iter()
				.map(|s| {
					let pair_info: PairInfo = s.clone().into(); // Convert FuturesSymbol to PairInfo
					let pair: Pair = s.symbol.try_into().expect("We assume v_utils is able to handle translating all Binance symbols");
					(pair, pair_info)
				})
				.collect(),
		}
	}
}
impl From<FuturesSymbol> for PairInfo {
	fn from(v: FuturesSymbol) -> Self {
		Self { price_precision: v.price_precision }
	}
}

//DO: transfer these to be embedded into the outputted struct, compiled for each symbol on acquisition.
//impl BinanceExchangeFutures {
//	pub fn min_notional(&self, symbol: Symbol) -> f64 {
//		let symbol_info = self.symbols.iter().find(|s| s.symbol == symbol.ticker()).unwrap();
//		let min_notional = symbol_info.filters.iter().find(|f| f["filterType"] == "MIN_NOTIONAL").unwrap();
//		min_notional["minNotional"].as_str().unwrap().parse().unwrap()
//	}
//
//	pub fn pair(&self, base_asset: &str, quote_asset: &str) -> Option<&FuturesSymbol> {
//		//? Should I cast `to_uppercase()`?
//		self.symbols.iter().find(|s| s.base_asset == base_asset && s.quote_asset == quote_asset)
//	}
//}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RateLimit {
	pub interval: String,
	pub interval_num: u32,
	pub limit: u32,
	pub rate_limit_type: String,
}

// the thing with multiplying orders due to weird limits should be here.
//#[derive(Debug, Deserialize, Serialize)]
//#[allow(non_snake_case)]
// struct SymbolFilter {
// 	filterType: String,
// 	maxPrice: String,
// 	minPrice: String,
// 	tickSize: String,
// 	maxQty: String,
// 	minQty: String,
// 	stepSize: String,
// 	limit: u32,
// 	notional: String,
// 	multiplierUp: String,
// 	multiplierDown: String,
// 	multiplierDecimal: u32,
//}

#[serde_as]
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FuturesSymbol {
	pub symbol: String,
	pub pair: String,
	pub contract_type: String,
	pub delivery_date: i64,
	pub onboard_date: i64,
	pub status: String,
	pub base_asset: String,
	pub quote_asset: String,
	pub margin_asset: String,
	pub price_precision: u8,
	pub quantity_precision: u32,
	pub base_asset_precision: u32,
	pub quote_precision: u32,
	pub underlying_type: String,
	pub underlying_sub_type: Vec<String>,
	pub settle_plan: Option<u32>,
	pub trigger_protect: String,
	pub filters: Vec<Value>,
	pub order_type: Option<Vec<String>>,
	pub time_in_force: Vec<String>,
	#[serde_as(as = "DisplayFromStr")]
	pub liquidation_fee: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub market_take_bound: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "filterType")]
pub enum Filter {
	#[serde(rename = "PRICE_FILTER")]
	PriceFilter(PriceFilter),
	#[serde(rename = "LOT_SIZE")]
	LotSize(LotSizeFilter),
	#[serde(rename = "MARKET_LOT_SIZE")]
	MarketLotSize(MarketLotSizeFilter),
	#[serde(rename = "MAX_NUM_ORDERS")]
	MaxNumOrders(MaxNumOrdersFilter),
	#[serde(rename = "MAX_NUM_ALGO_ORDERS")]
	MaxNumAlgoOrders(MaxNumAlgoOrdersFilter),
	#[serde(rename = "MIN_NOTIONAL")]
	MinNotional(MinNotionalFilter),
	#[serde(rename = "PERCENT_PRICE")]
	PercentPrice(PercentPriceFilter),
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceFilter {
	#[serde_as(as = "DisplayFromStr")]
	pub min_price: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub max_price: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub tick_size: f64,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LotSizeFilter {
	#[serde_as(as = "DisplayFromStr")]
	pub max_qty: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub min_qty: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub step_size: f64,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketLotSizeFilter {
	#[serde_as(as = "DisplayFromStr")]
	pub max_qty: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub min_qty: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub step_size: f64,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MaxNumOrdersFilter {
	pub limit: u32,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MaxNumAlgoOrdersFilter {
	pub limit: u32,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MinNotionalFilter {
	#[serde_as(as = "DisplayFromStr")]
	pub notional: f64,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PercentPriceFilter {
	#[serde_as(as = "DisplayFromStr")]
	pub multiplier_up: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub multiplier_down: f64,
	pub multiplier_decimal: u8,
}

impl FuturesSymbol {
	fn get_filter<T: for<'de> Deserialize<'de>>(&self, filter_type: &str) -> Option<T> {
		self.filters.iter().find_map(|filter| {
			if filter["filterType"] == filter_type {
				serde_json::from_value(filter.clone()).ok()
			} else {
				None
			}
		})
	}

	pub fn price_filter(&self) -> Option<PriceFilter> {
		self.get_filter("PRICE_FILTER")
	}

	pub fn lot_size_filter(&self) -> Option<LotSizeFilter> {
		self.get_filter("LOT_SIZE")
	}

	pub fn market_lot_size_filter(&self) -> Option<MarketLotSizeFilter> {
		self.get_filter("MARKET_LOT_SIZE")
	}

	pub fn max_num_orders_filter(&self) -> Option<MaxNumOrdersFilter> {
		self.get_filter("MAX_NUM_ORDERS")
	}

	pub fn max_num_algo_orders_filter(&self) -> Option<MaxNumAlgoOrdersFilter> {
		self.get_filter("MAX_NUM_ALGO_ORDERS")
	}

	pub fn min_notional_filter(&self) -> Option<MinNotionalFilter> {
		self.get_filter("MIN_NOTIONAL")
	}

	pub fn percent_price_filter(&self) -> Option<PercentPriceFilter> {
		self.get_filter("PERCENT_PRICE")
	}
}

#[cfg(test)]
mod tests {
	use serde_json::json;

	use super::*;

	#[serde_as]
	#[derive(Debug, Deserialize, Serialize)]
	#[serde(rename_all = "camelCase")]
	struct MiniSymbol {
		price_precision: u8,
		quantity_precision: u8,
		quote_asset: String,
		quote_precision: u8,
		#[serde_as(as = "DisplayFromStr")]
		required_margin_percent: f64,
		#[serde(default)]
		settle_plan: Option<u32>,
		status: String,
		symbol: String,
		time_in_force: Vec<String>,
		#[serde_as(as = "DisplayFromStr")]
		trigger_protect: f64,
		underlying_sub_type: Vec<String>,
		underlying_type: String,
	}

	#[test]
	fn mini_symbol() {
		let json = json!({
			"pricePrecision": 2,
			"quantityPrecision": 3,
			"quoteAsset": "USDT",
			"quotePrecision": 8,
			"requiredMarginPercent": "5.0000",  // Needs to be a string
			"settlePlan": null,
			"status": "TRADING",
			"symbol": "BTCUSDT",
			"timeInForce": [
				"GTC",
				"IOC",
				"FOK",
				"GTX",
				"GTD"
			],
			"triggerProtect": "0.0500",  // Needs to be a string
			"underlyingSubType": [
				"PoW"
			],
			"underlyingType": "COIN"
		});

		let _: MiniSymbol = serde_json::from_value(json).unwrap();
	}

	#[test]
	fn futures_symbol() {
		let json = json!({
			"baseAsset": "BTC",
			"baseAssetPrecision": 8,
			"contractType": "PERPETUAL",
			"deliveryDate": 4133404800000_i64,
			"filters": [
				{
					"filterType": "PRICE_FILTER",
					"maxPrice": "4529764",
					"minPrice": "556.80",
					"tickSize": "0.10"
				},
				{
					"filterType": "LOT_SIZE",
					"maxQty": "1000",
					"minQty": "0.001",
					"stepSize": "0.001"
				},
				{
					"filterType": "MARKET_LOT_SIZE",
					"maxQty": "120",
					"minQty": "0.001",
					"stepSize": "0.001"
				},
				{
					"filterType": "MAX_NUM_ORDERS",
					"limit": 200
				},
				{
					"filterType": "MAX_NUM_ALGO_ORDERS",
					"limit": 10
				},
				{
					"filterType": "MIN_NOTIONAL",
					"notional": "100"
				},
				{
					"filterType": "PERCENT_PRICE",
					"multiplierDecimal": "4",
					"multiplierDown": "0.9500",
					"multiplierUp": "1.0500"
				}
			],
			"liquidationFee": "0.012500",  // Needs to be a string
			"maintMarginPercent": "2.5000", // Needs to be a string
			"marginAsset": "USDT",
			"marketTakeBound": "0.05",      // Needs to be a string
			"maxMoveOrderLimit": 10000,
			"onboardDate": 1569398400000_i64,
			"orderTypes": [
				"LIMIT",
				"MARKET",
				"STOP",
				"STOP_MARKET",
				"TAKE_PROFIT",
				"TAKE_PROFIT_MARKET",
				"TRAILING_STOP_MARKET"
			],
			"pair": "BTCUSDT",
			"pricePrecision": 2,
			"quantityPrecision": 3,
			"quoteAsset": "USDT",
			"quotePrecision": 8,
			"requiredMarginPercent": "5.0000",  // Needs to be a string
			"status": "TRADING",
			"symbol": "BTCUSDT",
			"timeInForce": [
				"GTC",
				"IOC",
				"FOK",
				"GTX",
				"GTD"
			],
			"triggerProtect": "0.0500",  // Needs to be a string
			"underlyingSubType": [
				"PoW"
			],
			"underlyingType": "COIN"
		});

		let _: FuturesSymbol = serde_json::from_value(json).unwrap();
	}
}
