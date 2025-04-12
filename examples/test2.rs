#![feature(try_blocks)]
use std::{env, vec};

use v_exchanges_adapters::bybit::{BybitOption, BybitWsUrlBase};

fn main() {
	v_utils::clientside!();

	let rt = tokio::runtime::Runtime::new().unwrap();
	rt.block_on(async {
		run().await;
	});
}

pub async fn run() {
	let topics = vec!["publicTrade.BTCUSDT".to_owned()];
	let client = v_exchanges_adapters::Client::default();
	let mut ws_connection = client.ws_connection("/v5/public/linear", vec![BybitOption::WsUrl(BybitWsUrlBase::Bybit), BybitOption::WsTopics(topics)]);
	loop {
		let v = ws_connection.next().await.unwrap();
		println!("{v:#?}");
	}
}

pub async fn run_authed() {
	let pubkey = env::var("BINANCE_TIGER_FULL_PUBKEY").unwrap();
	let secret = env::var("BINANCE_TIGER_FULL_SECRET").unwrap();

	let client = v_exchanges_adapters::Client::default();
	let topics = vec!["position".to_owned()];
	let mut ws_connection = client.ws_connection(
		"/v5/private",
		vec![
			BybitOption::Pubkey(pubkey),
			BybitOption::Secret(secret.into()),
			BybitOption::WsAuth(true),
			BybitOption::WsUrl(BybitWsUrlBase::Bybit),
			BybitOption::WsTopics(topics),
		],
	);

	loop {
		let v = ws_connection.next().await.unwrap();
		println!("{v:#?}");
	}
}
