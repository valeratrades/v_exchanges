use serde::Serialize;
use v_exchanges_adapters::{
	Client,
	binance::{BinanceHttpUrl, BinanceOption},
};
mod binance;

use binance::futures::KlineCore;
use v_utils::utils::{LogDestination, init_subscriber};

//- [ ] generics request for klines rest
//- [ ] generics request for klines ws
// just start subbing stuff

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	init_subscriber(LogDestination::xdg_data_home("v_exchanges"));

	tracing::info!("Starting...");
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

	// Vec of: [1731448080000,\"88591.90\",\"88630.90\",\"88560.00\",\"88574.10\",\"173.581\",1731448139999,\"15378315.48720\",2800,\"113.654\",\"10069629.84420\",\"0\"]
	// https://binance-docs.github.io/apidocs/futures/en/#kline-candlestick-data
	let klines: Vec<KlineCore> = client.get("/fapi/v1/klines", Some(&KlineParams::default()), [BinanceOption::Default]).await.unwrap();

	dbg!(&klines);
}
