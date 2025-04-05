#![feature(try_blocks)]
use std::{
	env,
	time::{SystemTime, UNIX_EPOCH},
	vec,
};

use eyre::Result;
use hmac::{Hmac, Mac as _};
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::json;
use sha2::Sha256;
use tracing::instrument;
use v_exchanges_adapters::{
	bybit::{BybitOption, BybitWsUrlBase},
	generics::{
		http::{HeaderMap, header::HeaderValue},
		reqwest::{self, Url},
		tokio_tungstenite::tungstenite,
		ws::{WsConfig, WsConnection, WsError, WsHandler},
	},
};

fn main() {
	v_utils::clientside!();

	let rt = tokio::runtime::Runtime::new().unwrap();
	rt.block_on(async {
		run().await;
	});
}

//TODO: switch ot a private endpoint for Binance now
async fn run() {
	let pubkey = env::var("BINANCE_TIGER_FULL_PUBKEY").unwrap();
	let secret = env::var("BINANCE_TIGER_FULL_SECRET").unwrap();

	let client = v_exchanges_adapters::Client::default();
	let topics = vec!["position".to_owned()];
	let mut ws_connection = client.ws_connection(
		"/v5/private",
		vec![
			BybitOption::Pubkey(pubkey),
			BybitOption::Secret(secret.into()),
			/*BybitOption::WsAuth(true),*/ BybitOption::WsUrl(BybitWsUrlBase::Bybit),
			BybitOption::WsTopics(topics),
		],
	);

	loop {
		let v = ws_connection.next().await.unwrap();
		println!("{v:#?}");
	}
}
