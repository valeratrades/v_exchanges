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

async fn run() {
	let pubkey = env::var("BYBIT_TIGER_FULL_PUBKEY").unwrap();
	let secret = env::var("BYBIT_TIGER_FULL_SECRET").unwrap();

	{
		let pos = get_nonzero_positions(&pubkey, &secret).await.unwrap();
		println!("{:#}", pos);
	}

	let client = v_exchanges_adapters::Client::default();
	let topics = vec!["position".to_owned()];
	let mut ws_connection = client.ws_connection(
		"/v5/private",
		vec![BybitOption::Pubkey(pubkey), BybitOption::Secret(secret.into()), /*BybitOption::WsAuth(true),*/ BybitOption::WsUrl(BybitWsUrlBase::Bybit), BybitOption::WsTopics(topics)],
	);

	loop {
		let v = ws_connection.next().await.unwrap();
		println!("{v:#?}");
	}
}

async fn get_nonzero_positions(pubkey: &str, secret: &str) -> Result<String> {
	// Extract URL components
	let base_url = "https://api.bybit.com/v5/position/list";
	let params = "category=linear&settleCoin=USDT";

	let client = reqwest::Client::new();
	let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;

	// Use the signature format from the error message:
	// timestamp + api_key + recv_window + query_params
	let recv_window = "5000";
	let sign_str = format!("{}{}{}{}", timestamp, pubkey, recv_window, params);

	let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())?;
	mac.update(sign_str.as_bytes());
	let signature = hex::encode(mac.finalize().into_bytes());

	let mut headers = HeaderMap::new();
	headers.insert("X-BAPI-API-KEY", HeaderValue::from_str(pubkey)?);
	headers.insert("X-BAPI-SIGN", HeaderValue::from_str(&signature)?);
	headers.insert("X-BAPI-TIMESTAMP", HeaderValue::from_str(&timestamp.to_string())?);
	headers.insert("X-BAPI-RECV-WINDOW", HeaderValue::from_str(recv_window)?);

	let response = client.get(format!("{}?{}", base_url, params)).headers(headers).send().await?;

	Ok(response.text().await?)
}
