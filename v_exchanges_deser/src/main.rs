//NB: this whole main is for testing purposes only, I will eventually just expose the actual lib.rs once all is good.
use serde::Serialize;
use v_exchanges_adapters::{
	Client,
	binance::{BinanceHttpUrl, BinanceOption},
};
mod binance;
use v_utils::{
	trades::Timeframe,
	utils::{LogDestination, init_subscriber},
};

//- [ ] generics request for klines rest
//- [ ] generics request for klines ws
// just start subbing stuff

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	init_subscriber(LogDestination::xdg("v_exchanges"));

	let mut client = Client::new();
	client.update_default_option(BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM));

	let klines = binance::futures::market::klines(&client, ("BTC", "USDT").into(), "1m".into(), None, None, None).await.unwrap();
	dbg!(&klines);
}
