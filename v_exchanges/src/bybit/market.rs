use std::collections::VecDeque;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges_adapters::bybit::BybitOption;
use v_utils::{
	trades::{Kline, Ohlc, Pair},
	utils::filter_nulls,
};

use super::{BybitInterval, BybitIntervalTime};
use crate::{
	ExchangeName, ExchangeResult, Instrument, Symbol,
	core::{Klines, OpenInterest, RequestRange},
};

// klines {{{
pub(super) async fn klines(client: &v_exchanges_adapters::Client, symbol: Symbol, tf: BybitInterval, range: RequestRange) -> ExchangeResult<Klines> {
	range.ensure_allowed(1..=1000, &tf)?;
	let range_json = range.serialize(ExchangeName::Bybit);
	let base_params = filter_nulls(json!({
		"category": "linear", // can be ["linear", "inverse", "spot"] afaiu, could drive some generics with this later, but for now hardcode
		"symbol": symbol.pair.fmt_bybit(),
		"interval": tf.to_string(),
	}));

	let mut base_map = base_params.as_object().unwrap().clone();
	let range_map = range_json.as_object().unwrap();
	base_map.extend(range_map.clone());
	let params = filter_nulls(serde_json::Value::Object(base_map));

	let options = vec![BybitOption::None];
	let kline_response: KlineResponse = client.get("/v5/market/kline", &params, options).await.unwrap();

	let mut klines = VecDeque::with_capacity(kline_response.result.list.len());
	for k in kline_response.result.list {
		if kline_response.time > k.0 + tf.duration().as_millis() as i64
		/*take `as_millis`, so ok to downcast in all practical applications*/
		{
			klines.push_back(Kline {
				open_time: Timestamp::from_millisecond(k.0).unwrap(),
				ohlc: Ohlc {
					open: k.1,
					close: k.2,
					high: k.3,
					low: k.4,
				},
				volume_quote: k.5,
				trades: None,
				taker_buy_volume_quote: None,
			});
		}
	}
	Ok(Klines::new(klines, *tf))
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KlineResponse {
	pub result: ResponseResult,
	pub ret_code: i32,
	pub ret_ext_info: std::collections::HashMap<String, serde_json::Value>,
	pub ret_msg: String,
	pub time: i64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseResult {
	pub category: String,
	pub list: Vec<KlineData>,
	pub symbol: String,
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
pub struct KlineData(
	#[serde_as(as = "DisplayFromStr")] pub i64,
	#[serde_as(as = "DisplayFromStr")] pub f64,
	#[serde_as(as = "DisplayFromStr")] pub f64,
	#[serde_as(as = "DisplayFromStr")] pub f64,
	#[serde_as(as = "DisplayFromStr")] pub f64,
	#[serde_as(as = "DisplayFromStr")] pub f64,
	#[serde_as(as = "DisplayFromStr")] pub f64,
);
//,}}}

// price {{{
pub(super) async fn price(client: &v_exchanges_adapters::Client, pair: Pair) -> ExchangeResult<f64> {
	let params = filter_nulls(json!({
		"category": "linear",
		"symbol": pair.fmt_bybit(),
	}));
	let options = vec![BybitOption::None];
	let response: MarketTickerResponse = client.get("/v5/market/tickers", &params, options).await?;
	Ok(response.result.list[0].last_price)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketTickerResponse {
	pub ret_code: i32,
	pub ret_msg: String,
	pub result: MarketTickerResult,
	pub ret_ext_info: std::collections::HashMap<String, Value>,
	pub time: i64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketTickerResult {
	pub category: String,
	pub list: Vec<MarketTickerData>,
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketTickerData {
	pub symbol: String,
	#[serde_as(as = "DisplayFromStr")]
	pub last_price: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub index_price: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub mark_price: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub prev_price24h: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub price24h_pcnt: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub high_price24h: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub low_price24h: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub prev_price1h: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub open_interest: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub open_interest_value: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub turnover24h: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub volume24h: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub funding_rate: f64,
	pub next_funding_time: String,
	#[serde_as(as = "DisplayFromStr")]
	pub bid1_price: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub bid1_size: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub ask1_price: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub ask1_size: f64,
}
//,}}}

// open_interest {{{
pub(super) async fn open_interest(client: &v_exchanges_adapters::Client, symbol: Symbol, tf: BybitIntervalTime, range: RequestRange) -> ExchangeResult<Vec<OpenInterest>> {
	range.ensure_allowed(1..=200, &tf)?;
	let range_json = range.serialize(ExchangeName::Bybit);

	let base_params = filter_nulls(json!({
		"category": "linear",
		"symbol": symbol.pair.fmt_bybit(),
		"intervalTime": tf.to_string(),
	}));

	let mut base_map = base_params.as_object().unwrap().clone();
	let range_map = range_json.as_object().unwrap();
	base_map.extend(range_map.clone());
	let params = filter_nulls(serde_json::Value::Object(base_map));

	let options = vec![BybitOption::None];
	let response: OpenInterestResponse = client.get("/v5/market/open-interest", &params, options).await?;

	if response.result.list.is_empty() {
		return Err(crate::ExchangeError::Other(eyre::eyre!("No open interest data returned")));
	}

	// For PerpInverse, we need to fetch the price to convert
	let price = if symbol.instrument == Instrument::PerpInverse {
		let params = filter_nulls(json!({
			"category": "linear",
			"symbol": symbol.pair.fmt_bybit(),
		}));
		let options = vec![BybitOption::None];
		let ticker_response: MarketTickerResponse = client.get("/v5/market/tickers", &params, options).await?;
		Some(ticker_response.result.list[0].last_price)
	} else {
		None
	};

	// Convert all data points to OpenInterest
	let mut result = Vec::with_capacity(response.result.list.len());
	for data in response.result.list {
		let (val_asset, val_quote) = match symbol.instrument {
			Instrument::PerpInverse => {
				// as of (2025/10/14), Bybit returns value in `quote` for Inverse and in `asset` for Linear reqs
				let val_quote = data.open_interest;
				let price = price.expect("price should be set for PerpInverse");
				let val_asset = val_quote / price;
				(val_asset, Some(val_quote))
			}
			Instrument::Perp => (data.open_interest, None),
			_ => unreachable!(),
		};

		result.push(OpenInterest {
			val_asset,
			val_quote,
			timestamp: Timestamp::from_millisecond(data.timestamp).unwrap(),
			..Default::default()
		});
	}

	Ok(result)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenInterestResponse {
	pub ret_code: i32,
	pub ret_msg: String,
	pub result: OpenInterestResult,
	pub ret_ext_info: std::collections::HashMap<String, Value>,
	pub time: i64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenInterestResult {
	pub symbol: String,
	pub category: String,
	pub list: Vec<OpenInterestData>,
	pub next_page_cursor: String,
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenInterestData {
	#[serde_as(as = "DisplayFromStr")]
	pub open_interest: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub timestamp: i64,
}
//,}}}
