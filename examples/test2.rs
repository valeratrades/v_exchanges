#![feature(try_blocks)]
use std::{
	env,
	vec,
};

use futures_util::StreamExt as _;
use v_exchanges_adapters::binance::{BinanceAuth, BinanceOption};
use v_utils::prelude::*;

//#[derive(Clone, derive_more::Debug)]
//struct BybitWsHandler {
//	pubkey: String,
//	#[debug("[REDACTED]")]
//	secret: SecretString,
//	topics: Vec<String>,
//	auth: bool,
//}
//impl BybitWsHandler {
//	#[inline(always)]
//	fn subscribe_messages(&self) -> Vec<tungstenite::Message> {
//		vec![tungstenite::Message::Text(json!({ "op": "subscribe", "args": self.topics }).to_string().into())]
//	}
//}
//impl WsHandler for BybitWsHandler {
//	fn handle_start(&mut self) -> Vec<tungstenite::Message> {
//		if self.auth {
//			let pubkey = self.pubkey.clone();
//			let secret = self.secret.expose_secret().to_owned();
//
//			let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap(); // always after the epoch
//			let expires = time.as_millis() as u64 + 1000;
//
//			let mut hmac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap(); // hmac accepts key of any length
//
//			hmac.update(format!("GET/realtime{expires}").as_bytes());
//			let signature = hex::encode(hmac.finalize().into_bytes());
//
//			return vec![tungstenite::Message::Text(
//				json!({
//					"op": "auth",
//					"args": [pubkey, expires, signature],
//				})
//				.to_string()
//				.into(),
//			)];
//		}
//		self.subscribe_messages()
//	}
//
//	fn handle_message(&mut self, message: &serde_json::Value) -> Option<Vec<tungstenite::Message>> {
//		match message["op"].as_str() {
//			Some("auth") => {
//				if message["success"].as_bool() == Some(true) {
//					tracing::debug!("WebSocket authentication successful");
//				} else {
//					tracing::debug!("WebSocket authentication unsuccessful; message: {}", message["ret_msg"]);
//				}
//				Some(self.subscribe_messages())
//			}
//			Some("subscribe") => {
//				if message["success"].as_bool() == Some(true) {
//					tracing::debug!("WebSocket topics subscription successful");
//				} else {
//					tracing::debug!("WebSocket topics subscription unsuccessful; message: {}", message["ret_msg"]);
//				}
//				None
//			}
//			_ => None,
//		}
//	}
//}

#[tokio::main]
async fn main() {
	clientside!();

	let bn_url = "wss://stream.binance.com:443/ws/btcusdt@trade";
	//let bn_url = "wss://stream.binance.com:443/ws/btcusiaednt@trade"; //binance error
	//let bn_url = "wss://strbinance.com:443/ws/btcusiaednt@trade"; //connection error
	let client = v_exchanges_adapters::Client::default();
	let mut ws_connection = client.ws_connection(bn_url, vec![BinanceOption::HttpAuth(BinanceAuth::None)]);
	let mut i = 0;
	while let Ok(trade_event) = ws_connection.next().await {
		println!("{trade_event:?}");
		i += 1;
		if i > 10 {
			break;
		}
	}
	println!("\ngonna request reeconnect\n");
	ws_connection.request_reconnect().await.unwrap();
	println!("\nran request reconnect\n");

	while let Ok(trade_event) = ws_connection.next().await {
		println!("{trade_event:?}");
		i += 1;
		if i > 20 {
			break;
		}
	}

	//let bb_url = "wss://stream.bybit.com/v5/private";
	//
	//let handler = BybitWsHandler {
	//	pubkey: env::var("BYBIT_TIGER_READ_PUBKEY").unwrap(),
	//	secret: SecretString::new(env::var("BYBIT_TIGER_READ_SECRET").unwrap().into()),
	//	//topics: vec!["wallet".to_owned()],
	//	topics: vec!["kline.30.BTCUSDT".to_owned()],
	//	auth: true,
	//};
}
