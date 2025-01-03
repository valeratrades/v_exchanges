use std::fmt;

use chrono::{DateTime, TimeZone, Utc};
use color_eyre::eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use serde_with::{DisplayFromStr, serde_as};
use thiserror::Error;
use v_exchanges_adapters::bybit::{BybitHttpUrl, BybitOption};
use v_utils::{
	trades::{Kline, Ohlc, Pair, Timeframe},
	utils::filter_nulls,
};

use crate::core::Klines;

//MOVE: centralized error module
#[derive(Debug)]
struct LimitOutOfRangeError {
	allowed: std::ops::RangeInclusive<u32>,
	provided: u32,
}
impl fmt::Display for LimitOutOfRangeError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "Limit out of range. Allowed: {:?}, provided: {}", self.allowed, self.provided)
	}
}
impl std::error::Error for LimitOutOfRangeError {}

pub async fn klines(client: &v_exchanges_adapters::Client, pair: Pair, tf: Timeframe, limit: u32, start_time: Option<u64>, end_time: Option<u64>) -> Result<Klines> {
	let range = 1..=1000;
	if !range.contains(&limit) {
		return Err(LimitOutOfRangeError { allowed: range, provided: limit }.into());
	}

	let mut params = filter_nulls(json!({
		"category": "linear", // can be ["linear", "inverse", "spot"] afaiu, could drive some generics with this later, but for now hardcode
		"symbol": pair.to_string(),
		"interval": tf.format_bybit()?,
		"limit": limit,
		"startTime": start_time,
		"endTime": end_time,
	}));
	let kline_response: KlineResponse = client.get("/v5/market/kline", &params, [BybitOption::Default]).await.unwrap();

	let mut klines = Vec::new();
	for k in kline_response.result.list {
		if kline_response.time > k.0 + tf.duration().num_milliseconds() {
			klines.push(Kline {
				open_time: Utc.timestamp_millis(k.0),
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
	Ok(Klines { v: klines, tf, oi: Vec::new() })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KlineResponse {
	pub result: ResponseResult,
	pub ret_code: i32,
	pub ret_ext_info: std::collections::HashMap<String, serde_json::Value>,
	pub ret_msg: String,
	pub time: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseResult {
	pub category: String,
	pub list: Vec<KlineData>,
	pub symbol: String,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct KlineData(
	#[serde_as(as = "DisplayFromStr")] pub i64,
	#[serde_as(as = "DisplayFromStr")] pub f64,
	#[serde_as(as = "DisplayFromStr")] pub f64,
	#[serde_as(as = "DisplayFromStr")] pub f64,
	#[serde_as(as = "DisplayFromStr")] pub f64,
	#[serde_as(as = "DisplayFromStr")] pub f64,
	#[serde_as(as = "DisplayFromStr")] pub f64,
);
