use std::env;

use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	println!("=== Exchange API Key Health Check ===\n");

	check_binance().await;
	check_bybit().await;
	check_mexc().await;

	println!("\n=== Health Check Complete ===");
}

async fn check_binance() {
	println!("🔍 Checking Binance...");

	let key_var = "BINANCE_TIGER_READ_PUBKEY";
	let secret_var = "BINANCE_TIGER_READ_SECRET";

	match (env::var(key_var), env::var(secret_var)) {
		(Ok(key), Ok(secret)) => {
			let mut binance = ExchangeName::Binance.init_client();
			binance.auth(key, secret.into());

			match binance.balances(Instrument::Perp, Some(5000)).await {
				Ok(_) => println!("✅ Binance: API key is valid and active"),
				Err(e) => println!("❌ Binance: API key error - {}", e),
			}
		}
		_ => println!("⚠️  Binance: Environment variables {} or {} not set", key_var, secret_var),
	}
}

async fn check_bybit() {
	println!("🔍 Checking Bybit...");

	let key_var = "QUANTM_BYBIT_SUB_PUBKEY";
	let secret_var = "QUANTM_BYBIT_SUB_SECRET";

	match (env::var(key_var), env::var(secret_var)) {
		(Ok(key), Ok(secret)) => {
			let mut bybit = ExchangeName::Bybit.init_client();
			bybit.auth(key, secret.into());

			match bybit.balances(Instrument::Perp, Some(5000)).await {
				Ok(_) => println!("✅ Bybit: API key is valid and active"),
				Err(e) => println!("❌ Bybit: API key error - {}", e),
			}
		}
		_ => println!("⚠️  Bybit: Environment variables {} or {} not set", key_var, secret_var),
	}
}

async fn check_mexc() {
	println!("🔍 Checking MEXC...");

	let key_var = "MEXC_READ_KEY";
	let secret_var = "MEXC_READ_SECRET";

	match (env::var(key_var), env::var(secret_var)) {
		(Ok(key), Ok(secret)) => {
			let mut mexc = ExchangeName::Mexc.init_client();
			mexc.auth(key, secret.into());

			match mexc.balances(Instrument::Perp, Some(5000)).await {
				Ok(_) => println!("✅ MEXC: API key is valid and active"),
				Err(e) => {
					let err_str = e.to_string();
					if err_str.contains("API KEY 已过期") || err_str.contains("402") {
						println!("❌ MEXC: API key has expired");
					} else if err_str.contains("需要资产信息读取权限") || err_str.contains("701") {
						println!("❌ MEXC: API key lacks read permissions for account balance");
					} else {
						println!("❌ MEXC: API key error - {}", e);
					}
				}
			}
		}
		_ => println!("⚠️  MEXC: Environment variables {} or {} not set", key_var, secret_var),
	}
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
