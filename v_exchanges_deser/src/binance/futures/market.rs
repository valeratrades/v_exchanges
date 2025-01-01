use chrono::{DateTime, TimeZone, Utc};
//HACK: Methods should be implemented on the central interface struct, following <https://github.com/wisespace-io/binance-rs>.
use color_eyre::eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges_adapters::binance::{BinanceHttpUrl, BinanceOption};
use v_utils::{
	trades::{Kline, Ohlc, Pair, Timeframe},
	utils::filter_nulls,
};

use crate::core::Klines;

// klines {{{
pub async fn klines(client: &v_exchanges_adapters::Client, pair: Pair, tf: Timeframe, limit: u32, start_time: Option<u64>, end_time: Option<u64>) -> Result<Klines> {
	let mut params = filter_nulls(json!({
		"symbol": pair.to_string(),
		"interval": tf.format_binance()?,
		"limit": limit,
		"startTime": start_time,
		"endTime": end_time,
	}));

	let kline_responses: Vec<KlineResponse> = client.get("/fapi/v1/klines", &params, [BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM)]).await.unwrap();
	let klines: Vec<Kline> = kline_responses.into_iter().map(Kline::from).collect();

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
	pub close_time: i64,
	#[serde_as(as = "DisplayFromStr")]
	pub quote_asset_volume: f64,
	pub number_of_trades: usize,
	#[serde_as(as = "DisplayFromStr")]
	pub taker_buy_base_asset_volume: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub taker_buy_quote_asset_volume: f64,

	__ignore: Option<Value>,
}
impl From<KlineResponse> for Kline {
	fn from(k: KlineResponse) -> Self {
		let ohlc = Ohlc {
			open: k.open,
			high: k.high,
			low: k.low,
			close: k.close,
		};
		Kline {
			open_time: DateTime::from_timestamp_millis(k.open_time).unwrap(),
			ohlc,
			volume_quote: k.quote_asset_volume,
			//TODO!!!!!!: before adding check that it is not less than start_time + tf
			trades: Some(k.number_of_trades),
			taker_buy_volume_quote: Some(k.taker_buy_quote_asset_volume),
			close_time: Some(Utc.timestamp_millis_opt(k.close_time).unwrap()),
		}
	}
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
		let _: super::Kline = serde_json::from_str(raw_str).unwrap();
	}
}
