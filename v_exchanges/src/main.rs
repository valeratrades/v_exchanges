use rust_decimal::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use v_exchanges_adapters::{
	Client,
	binance::{BinanceHttpUrl, BinanceOption},
};
mod binance;

use binance::futures::KlineCore;
use v_utils::utils::init_subscriber;

//- [ ] generics request for klines rest
//- [ ] generics request for klines ws
// just start subbing stuff

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	init_subscriber(None);

	let mut client = Client::new();
	client.update_default_option(BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM));

	#[derive(Serialize)]
	pub struct KlineParams<'a> {
		pub symbol: &'a str,
		pub interval: &'a str,
		#[serde(skip_serializing_if = "Option::is_none")]
		pub limit: Option<u16>,
		#[serde(rename = "startTime", skip_serializing_if = "Option::is_none")]
		pub start_time: Option<u64>,
		#[serde(rename = "endTime", skip_serializing_if = "Option::is_none")]
		pub end_time: Option<u64>,
	}
	impl Default for KlineParams<'_> {
		fn default() -> Self {
			Self {
				symbol: "BTCUSDT",
				interval: "1m", //HACK: should use [v_exchangse_core::Timeframe] struct
				limit: None,
				start_time: None,
				end_time: None,
			}
		}
	}

	let klines: Vec<KlineCore> = client.get(/*https://fapi.binance.com*/"/fapi/v1/klines", Some(&KlineParams::default()), [BinanceOption::Default]).await.unwrap();

	dbg!(&klines);
}
