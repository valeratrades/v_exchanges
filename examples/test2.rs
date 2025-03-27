use std::{env, time::Duration};

use tracing::log::LevelFilter;
use v_exchanges::AbsMarket;
use v_exchanges_adapters::{
	Client,
	binance::{BinanceAuth, BinanceHttpUrl, BinanceOption, BinanceWebSocketUrl},
};

#[tokio::main]
async fn main() {
	println!("Hello world");
	let am: AbsMarket = "Binance/Futures".into();

	let client = am.client();
}
