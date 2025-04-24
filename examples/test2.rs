#![feature(try_blocks)]
use std::{collections::HashSet, env, vec};

use futures_util::StreamExt;
//use futures_util::StreamExt;
use v_exchanges_adapters::{
	generics::ws::WsConnection,
};

fn main() {
	v_utils::clientside!();

	let rt = tokio::runtime::Runtime::new().unwrap();
	rt.block_on(async {
		run_bybit().await;
		//foo().await;
	});
}

pub async fn run_bybit() {
	use v_exchanges_adapters::bybit::{BybitOption, BybitWsHandler, BybitWsUrlBase};
	let topics = vec!["publicTrade.BTCUSDT".to_owned()];
	let client = v_exchanges_adapters::Client::default();
	let mut ws_connection = client.ws_connection("/v5/public/linear", vec![BybitOption::WsUrl(BybitWsUrlBase::Bybit), BybitOption::WsTopics(topics)]);
	println!("Running as StreamExt::next()");
	loop {
		let v = ws_connection.next().await.unwrap();
		println!("{v:#?}");
	}
}

//pub async fn run_binance() {
//	use v_exchanges_adapters::binance::{BinanceOption, BinanceWsHandler, BinanceWsUrlBase};
//	let topics = vec!["publicTrade.BTCUSDT".to_owned()];
//	let client = v_exchanges_adapters::Client::default();
//	let mut ws_connection = client.ws_connection("/v5/public/linear", vec![BinanceOption::WsUrl(BinanceWsUrlBase::Binance), BinanceOption::WsTopics(topics)]);
//	println!("Running as StreamExt::next()");
//	loop {
//		let v = ws_connection.next().await.unwrap();
//		println!("{v:#?}");
//	}
//}

//pub async fn run_authed() {
//	let pubkey = env::var("BINANCE_TIGER_FULL_PUBKEY").unwrap();
//	let secret = env::var("BINANCE_TIGER_FULL_SECRET").unwrap();
//
//	let client = v_exchanges_adapters::Client::default();
//	let topics = vec!["position".to_owned()];
//	let mut ws_connection = client.ws_connection(
//		"/v5/private",
//		vec![
//			BybitOption::Pubkey(pubkey),
//			BybitOption::Secret(secret.into()),
//			BybitOption::WsAuth(true),
//			BybitOption::WsUrl(BybitWsUrlBase::Bybit),
//			BybitOption::WsTopics(topics),
//		],
//	);
//
//	loop {
//		let v = ws_connection.next().await.unwrap();
//		println!("{v:#?}");
//	}
//}
