//HACK: Methods should be implemented on the central interface struct, following <https://github.com/wisespace-io/binance-rs>.
use v_exchanges_adapters::binance::BinanceOption;

use color_eyre::eyre::Result;
use v_utils::trades::{Pair, Timeframe};
use serde::Serialize;

use crate::binance::futures::core::*;

//HACK: oversimplified
pub async fn klines(generic_client: &v_exchanges_adapters::Client, pair: Pair, tf: Timeframe, limit: Option<u16>, start_time: Option<u64>, end_time: Option<u64>) -> Result<Vec<Kline>> {
	#[derive(Serialize)]
	pub struct KlineParams {
		pub symbol: String,
		pub interval: String,
		#[serde(skip_serializing_if = "Option::is_none")]
		pub limit: Option<u16>,
		#[serde(rename = "startTime", skip_serializing_if = "Option::is_none")]
		pub start_time: Option<u64>,
		#[serde(rename = "endTime", skip_serializing_if = "Option::is_none")]
		pub end_time: Option<u64>,
	}

	let mut params = KlineParams {
		symbol: pair.to_string(),
		interval: tf.format_binance()?,
		limit,
		start_time,
		end_time,
	};

	//TODO!!!: have the function take the vec of options, instead of hardcoding `[BinanceOption::Default]`
	let klines: Vec<Kline> = generic_client.get("/fapi/v1/klines", Some(&params), [BinanceOption::Default]).await.unwrap();
	Ok(klines)
}
