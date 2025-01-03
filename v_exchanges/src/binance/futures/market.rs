use std::fmt;

use chrono::{DateTime, TimeZone, Utc};
//HACK: Methods should be implemented on the central interface struct, following <https://github.com/wisespace-io/binance-rs>.
use color_eyre::eyre::{self, Error, Result};
use color_eyre::eyre::{Report, eyre};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges_adapters::binance::{BinanceHttpUrl, BinanceOption};
use v_utils::{
	trades::{Kline, Ohlc, Pair, Timeframe},
	utils::filter_nulls,
};

use crate::core::{Klines, KlinesRequestRange};

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

// klines {{{
pub async fn klines(client: &v_exchanges_adapters::Client, pair: Pair, tf: Timeframe, range: KlinesRequestRange) -> Result<Klines> {
	let range_json = match range {
		KlinesRequestRange::StartEnd { start, end } => json!({
			"startTime": start.timestamp_millis(),
			"endTime": end.timestamp_millis(),
		}),
		KlinesRequestRange::Limit(limit) => {
			let allowed_range = 1..=1000;
			if !allowed_range.contains(&limit) {
				return Err(LimitOutOfRangeError {
					allowed: allowed_range,
					provided: limit,
				}
				.into());
			}
			json!({
				"limit": limit,
			})
		}
	};
	let mut base_params = filter_nulls(json!({
		"symbol": pair.to_string(),
		"interval": tf.format_binance()?,
	}));

	let mut base_map = base_params.as_object().unwrap().clone();
	let range_map = range_json.as_object().unwrap();
	base_map.extend(range_map.clone());
	let params = filter_nulls(serde_json::Value::Object(base_map));

	let kline_responses: Vec<KlineResponse> = client.get("/fapi/v1/klines", &params, [BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM)]).await.unwrap();

	let r_len = kline_responses.len();
	let mut klines = Vec::with_capacity(r_len);
	for (i, k) in kline_responses.into_iter().enumerate() {
		//HACK: have to check against [now](Utc::now) instead, because binance returns some dumb shit instead of actual close. Here structured this way in case they fix it in the future.
		let close_time = Utc::now().timestamp_millis();
		match close_time > k.open_time + (0.99 * tf.duration().num_milliseconds() as f64) as i64 {
			true => {
				let ohlc = Ohlc {
					open: k.open,
					high: k.high,
					low: k.low,
					close: k.close,
				};
				klines.push(Kline {
					open_time: DateTime::from_timestamp_millis(k.open_time).unwrap(),
					ohlc,
					volume_quote: k.quote_asset_volume,
					trades: Some(k.number_of_trades),
					taker_buy_volume_quote: Some(k.taker_buy_quote_asset_volume),
				});
			}
			false => match i == r_len - 1 {
				true => tracing::trace!("Skipped last kline in binance request, as it's incomplete (expected behavior)"),
				false => tracing::warn!("Skipped kline in binance request, as it's incomplete"),
			},
		}
	}
	Ok(Klines { v: klines, tf, oi: Vec::new() })
}

/** # Ex: ```json
[1731448080000,\"88591.90\",\"88630.90\",\"88560.00\",\"88574.10\",\"173.581\",1731448139999,\"15378315.48720\",2800,\"113.654\",\"10069629.84420\",\"0\"]
```
**/
#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KlineResponse {
	pub open_time: i64,
	#[serde_as(as = "DisplayFromStr")]
	pub open: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub close: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub high: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub low: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub volume: f64,
	/// As of today (2025/01/03), means NOTHING, as they will still send what it _SHOULD_ be even if the kline is not yet finished. (fuck you, binance)
	__close_time: i64,
	#[serde_as(as = "DisplayFromStr")]
	pub quote_asset_volume: f64,
	pub number_of_trades: usize,
	#[serde_as(as = "DisplayFromStr")]
	pub taker_buy_base_asset_volume: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub taker_buy_quote_asset_volume: f64,

	__ignore: Option<Value>,
}
//,}}}

// price {{{
//HACK: not sure this is _the_ thing to use for that (throwing away A LOT of data)
pub async fn price(client: &v_exchanges_adapters::Client, pair: Pair) -> Result<f64> {
	let mut params = json!({
		"symbol": pair.to_string(),
	});

	let r: MarkPriceResponse = client.get("/fapi/v1/premiumIndex", &params, [BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM)]).await.unwrap();
	let price = r.index_price; // when using this framework, we care for per-exchange price, obviously
	Ok(price)
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MarkPriceResponse {
	pub symbol: String,
	#[serde_as(as = "DisplayFromStr")]
	pub mark_price: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub index_price: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub estimated_settle_price: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub last_funding_rate: f64,
	pub next_funding_time: u64,
	pub time: u64,
}

//,}}}

#[cfg(test)]
mod tests {
	#[test]
	fn klines() {
		let raw_str = "[1731448080000,\"88591.90\",\"88630.90\",\"88560.00\",\"88574.10\",\"173.581\",1731448139999,\"15378315.48720\",2800,\"113.654\",\"10069629.84420\",\"0\"]";
		let _: super::KlineResponse = serde_json::from_str(raw_str).unwrap();
	}
}
