/// Exercises auth error interpretation for binance and bybit.
/// Sends requests with fake API keys and verifies we get `ApiError::Auth` variants back.
use v_exchanges::adapters::{
	Client,
	binance::{BinanceAuth, BinanceHttpUrl, BinanceOption},
	bybit::{BybitHttpAuth, BybitOption},
	generics::http::{ApiError, AuthError, HandleError, RequestError},
};

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let client = Client::default();

	// --- Binance: invalid API key ---
	println!("=== Binance: request with fake API key ===");
	let result: Result<serde_json::Value, RequestError> = client
		.get_no_query(
			"/api/v3/account",
			[
				BinanceOption::Pubkey("FAKE_EXPIRED_KEY_12345".to_string()),
				BinanceOption::Secret("FAKE_SECRET_67890".to_string().into()),
				BinanceOption::HttpUrl(BinanceHttpUrl::Spot),
				BinanceOption::HttpAuth(BinanceAuth::Sign),
			],
		)
		.await;

	match &result {
		Err(RequestError::HandleResponse(HandleError::Api(ApiError::Auth(auth_err)))) => {
			println!("  Got ApiError::Auth as expected!");
			match auth_err {
				AuthError::KeyExpired { msg } => println!("  KeyExpired: {msg}"),
				AuthError::Unauthorized { msg } => println!("  Unauthorized: {msg}"),
				other => println!("  Other auth error: {other}"),
			}
		}
		Err(e) => println!("  Got different error (may be expected if exchange returns different code): {e}"),
		Ok(v) => println!("  Unexpected success: {v}"),
	}

	// --- Bybit: invalid API key ---
	println!("\n=== Bybit: request with fake API key ===");
	let result: Result<serde_json::Value, RequestError> = client
		.get(
			"/v5/account/wallet-balance",
			&[("accountType", "UNIFIED")],
			[
				BybitOption::Pubkey("FAKE_EXPIRED_KEY_12345".to_string()),
				BybitOption::Secret("FAKE_SECRET_67890".to_string().into()),
				BybitOption::HttpAuth(BybitHttpAuth::V3AndAbove),
			],
		)
		.await;

	match &result {
		Err(RequestError::HandleResponse(HandleError::Api(ApiError::Auth(auth_err)))) => {
			println!("  Got ApiError::Auth as expected!");
			match auth_err {
				AuthError::KeyExpired { msg } => println!("  KeyExpired: {msg}"),
				AuthError::Unauthorized { msg } => println!("  Unauthorized: {msg}"),
				other => println!("  Other auth error: {other}"),
			}
		}
		Err(e) => println!("  Got different error (may be expected if exchange returns different code): {e}"),
		Ok(v) => println!("  Unexpected success: {v}"),
	}

	println!("\nDone.");
}
