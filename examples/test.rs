#![feature(try_blocks)]
use std::{env, time::SystemTime, vec};

use hmac::{Hmac, Mac as _};
use secrecy::{ExposeSecret as _, SecretString};
use sha2::Sha256;
use tracing::instrument;
use v_exchanges_adapters::generics::{
	http::{header::HeaderValue, HeaderMap}, reqwest::{self, Url}, tokio_tungstenite::tungstenite, ws::{WsConfig, WsConnection, WsError, WsHandler}
};
use std::time::UNIX_EPOCH;
use serde_json::json;
use eyre::Result;

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

	let handler = BybitWsHandler {
		pubkey,
		secret: secret.into(),
		topics: vec!["position".to_owned()],
		auth: true,
	};

	let mut ws_connection = WsConnection::new(
		"",
		handler
	);
	loop {
		let v = ws_connection.next().await.unwrap();
		println!("{v:#?}");
	}
}

#[derive(Clone, derive_more::Debug)]
struct BybitWsHandler {
	pubkey: String,
	#[debug("[REDACTED]")]
	secret: SecretString,
	topics: Vec<String>,
	auth: bool,
}
impl BybitWsHandler {
	#[inline(always)]
	fn subscribe_messages(&self) -> Vec<tungstenite::Message> {
		vec![tungstenite::Message::Text(json!({ "op": "subscribe", "args": self.topics }).to_string().into())]
	}
}
impl WsHandler for BybitWsHandler {
	fn config(&self) -> WsConfig {
		WsConfig {
			base_url: Some(Url::parse("wss://stream.bybit.com/v5/private").unwrap()), //dbg: private base
			..Default::default()
		}
	}

	fn handle_auth(&mut self) -> Result<Vec<tungstenite::Message>, WsError> {
		if self.auth {
			let pubkey = self.pubkey.clone();
			let secret = self.secret.clone();
			let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).expect("always after the epoch");
			//XXX: expiration time here is hardcoded to 1s, which would override any specifications of a longer recv_window on top.
			let expires = time.as_millis() as u64 + 1000; //TODO: figure out how large can I make this

			// sign with HMAC-SHA256
			let mut hmac = Hmac::<Sha256>::new_from_slice(secret.expose_secret().as_bytes()).expect("hmac accepts key of any length");
			hmac.update(format!("GET/realtime{expires}").as_bytes());
			let signature = hex::encode(hmac.finalize().into_bytes());

			return Ok(vec![tungstenite::Message::Text(
				json!({
					"op": "auth",
					"args": [pubkey, expires, signature],
				})
					.to_string()
					.into(),
			)]);
		}
		Ok(self.subscribe_messages())
	}

	#[instrument(skip_all, fields(jrpc = ?format_args!("{:#?}", jrpc)))]
	fn handle_message(&mut self, jrpc: &serde_json::Value) -> Option<Vec<tungstenite::Message>> {
		match jrpc["op"].as_str() {
			Some("auth") => {
				if jrpc["success"].as_bool() == Some(true) {
					tracing::info!("WebSocket authentication successful");
				} else {
					tracing::warn!("WebSocket authentication unsuccessful");
				}
				Some(self.subscribe_messages())
			}
			Some("subscribe") => {
				if jrpc["success"].as_bool() == Some(true) {
					tracing::info!("WebSocket topics subscription successful");
				} else {
					tracing::warn!("WebSocket topics subscription unsuccessful");
				}
				Some(vec![]) // otherwise, if we return None here, with current implementanion (2025/04/04) we'd be accepting the message as containing desired content.
			}
			_ => None,
		}
	}
}

async fn get_nonzero_positions(pubkey: &str, secret: &str) -> Result<String> {
    // Extract URL components
    let base_url = "https://api.bybit.com/v5/position/list";
    let params = "category=linear&settleCoin=USDT";
    
    let client = reqwest::Client::new();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    
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

    let response = client
        .get(format!("{}?{}", base_url, params))
        .headers(headers)
        .send()
        .await?;

    Ok(response.text().await?)
}

