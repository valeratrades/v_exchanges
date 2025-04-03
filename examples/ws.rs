#![feature(try_blocks)]
use std::{env, vec};

use futures_util::StreamExt as _;
use v_exchanges_adapters::binance::{BinanceAuth, BinanceOption};
use v_utils::prelude::*;

#[tokio::main]
async fn main() {
	clientside!();

	//TODO: switch to a generic exchange declaration, to show that this is available for all of them.
	let binance = v_exchanges::binance::Binance::default();
	let mut rx = binance.ws_trade_futs(("BTC", "USDT").into()).await.unwrap();

	while let Some(trade_event) = rx.recv().await {
		println!("{trade_event:?}");
	}
}
