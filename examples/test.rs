#![feature(try_blocks)]
use std::{env, time::SystemTime, vec};

use hmac::{Hmac, Mac as _};
use secrecy::{ExposeSecret as _, SecretString};
use sha2::Sha256;
use v_exchanges_adapters::generics::{
	tokio_tungstenite::tungstenite,
	ws::{WsConfig, WsConnection, WsError, WsHandler},
};
use v_utils::prelude::*;

fn main() {
	clientside!();
	let rt = tokio::runtime::Runtime::new().unwrap();
	rt.block_on(async {
		run().await;
	});
}

async fn run() {
	//let bb_url = "wss://stream.binance.com:443/ws/btcusdt@trade";
	let bb_order_url_suffix = "wss://stream.binance.com:9443/ws/btcusdt@trade";
	//let client = v_exchanges_adapters::Client::default();

	//let mut ws_connection = client.ws_connection(bn_url, vec![BybitAuth::HttpAuth(BybitAuth::None)]);

	let handler = BybitWsHandler {
		pubkey: env::var("TIGER_BYBIT_FULL_PUBKEY").unwrap(),
		secret: env::var("TIGER_BYBIT_FULL_SECRET").unwrap().into(),
		topics: vec!["trade.BTCUSDT".to_string()],
		auth: true,
	};

	//let mut ws_connection = WsConnection::new(
	//	&handler,
	//	WsConfig {
	//		url: bb_order_url_suffix.to_string(),
	//		..Default::default()
	//	},
	//)
	//while let Ok(trade_event) = ws_connection.next().await {
	//	println!("{trade_event:?}");
	//	if trade_event["M"] == serde_json::Value::Bool(false) {
	//		unreachable!();
	//	}
	//}
	todo!();
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
	fn handle_start(&mut self, params: Option<serde_json::Value>) -> Result<Vec<tungstenite::Message>, WsError> {
		if self.auth {
			let pubkey = self.pubkey.clone();
			let secret = self.secret.expose_secret();

			let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap(); // always after the epoch
			let expires = time.as_millis() as u64 + 1000;

			let mut hmac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap(); // hmac accepts key of any length

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

	fn handle_message(&mut self, message: &serde_json::Value) -> Option<Vec<tungstenite::Message>> {
		match message["op"].as_str() {
			Some("auth") => {
				if message["success"].as_bool() == Some(true) {
					tracing::debug!("WebSocket authentication successful");
				} else {
					tracing::debug!("WebSocket authentication unsuccessful; message: {}", message["ret_msg"]);
				}
				Some(self.subscribe_messages())
			}
			Some("subscribe") => {
				if message["success"].as_bool() == Some(true) {
					tracing::debug!("WebSocket topics subscription successful");
				} else {
					tracing::debug!("WebSocket topics subscription unsuccessful; message: {}", message["ret_msg"]);
				}
				None
			}
			_ => None,
		}
	}
}
