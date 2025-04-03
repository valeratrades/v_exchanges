#![feature(try_blocks)]
use std::{env, vec};

use futures_util::StreamExt as _;
use v_exchanges_adapters::binance::{BinanceAuth, BinanceOption};
use v_utils::prelude::*;

#[tokio::main]
async fn main() {
	clientside!();

	//let bn_url = "wss://stream.binance.com:443/ws/btcusiaednt@trade"; //binance error
	//let bn_url = "wss://strbinance.com:443/ws/btcusiaednt@trade"; //connection error
	let binance = v_exchanges::binance::Binance::default();
	let mut rx = binance.ws_trade_futs(("BTC", "USDT").into()).await.unwrap();

	while let Some(trade_event) = rx.recv().await {
		println!("{trade_event:?}");
	}
}
