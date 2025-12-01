use std::env;

use v_exchanges_adapters::{
	bybit::{BybitOption, BybitOptions, BybitWsUrlBase},
	generics::ws::WsConnection,
	traits::{HandlerOptions, WsOption},
};

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let keys_prefix = "QUANTM_BYBIT_SUB";
	let pubkey_name = format!("{keys_prefix}_PUBKEY");
	let secret_name = format!("{keys_prefix}_SECRET");

	if let (Ok(pubkey), Ok(secret)) = (env::var(&pubkey_name), env::var(&secret_name)) {
		test_private_ws(pubkey, secret).await;
	} else {
		eprintln!("{pubkey_name} or {secret_name} is missing, cannot test private websocket methods.");
		eprintln!("Please set these environment variables to test Bybit private websocket functionality.");
		std::process::exit(1);
	}
}

async fn test_private_ws(pubkey: String, secret: String) {
	println!("Testing Bybit private websocket with wallet balance updates...");

	// Create options with authentication
	let mut options = vec![
		BybitOption::Pubkey(pubkey),
		BybitOption::Secret(secret.into()),
		BybitOption::WsUrl(BybitWsUrlBase::Bybit),
		BybitOption::WsAuth(true),                         // Enable authentication for private endpoints
		BybitOption::WsTopics(vec!["wallet".to_string()]), // Subscribe to wallet updates
	];

	// Create the websocket handler
	let handler = BybitOption::ws_handler(options.drain(..).fold(BybitOptions::default(), |mut opts, opt| {
		opts.update(opt);
		opts
	}));

	// Create websocket connection to the private endpoint
	// Bybit V5 uses /v5/private for private data streams
	let mut ws_connection = WsConnection::try_new("/v5/private", handler).expect("Failed to create WebSocket connection");

	println!("WebSocket connection established. Waiting for wallet balance updates...");
	println!("Note: You may need to make trades or transfers to see balance updates.");
	println!("Listening for the first 5 messages...");

	// Listen for wallet updates
	let mut message_count = 0;
	let max_messages = 5;

	while message_count < max_messages {
		match ws_connection.next().await {
			Ok(event) => {
				message_count += 1;
				println!("\n=== Message {} ===", message_count);
				println!("Topic: {}", event.topic);
				println!("Event Type: {}", event.event_type);
				println!("Timestamp: {}", event.time);
				println!("Data: {}", serde_json::to_string_pretty(&event.data).unwrap());

				// Try to parse the balance data
				if event.topic == "wallet" {
					if let Some(coins) = event.data.get("coin").and_then(|c: &serde_json::Value| c.as_array()) {
						println!("\nParsed Balance Information:");
						for coin in coins {
							if let (Some(coin_name), Some(wallet_balance), Some(usd_value)) = (
								coin.get("coin").and_then(|c: &serde_json::Value| c.as_str()),
								coin.get("walletBalance").and_then(|b: &serde_json::Value| b.as_str()),
								coin.get("usdValue").and_then(|u: &serde_json::Value| u.as_str()),
							) {
								println!("  {} - Balance: {} (USD: {})", coin_name, wallet_balance, usd_value);
							}
						}
					}
				}
			}
			Err(e) => {
				eprintln!("Error receiving message: {:?}", e);
				break;
			}
		}
	}

	println!("\nTest completed successfully!");
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	#[ignore] // Requires API credentials
	async fn test_bybit_private_websocket() {
		let pubkey = env::var("QUANTM_BYBIT_SUB_PUBKEY").expect("QUANTM_BYBIT_SUB_PUBKEY must be set");
		let secret = env::var("QUANTM_BYBIT_SUB_SECRET").expect("QUANTM_BYBIT_SUB_SECRET must be set");

		test_private_ws(pubkey, secret).await;
	}
}
