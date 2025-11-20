use std::{env, time::Duration};

use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	println!("=== Exchange API Key Health Check ===\n");

	check_binance().await;
	check_bybit().await;
	check_kucoin().await;
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

			match binance.balances(Instrument::Perp, Some(Duration::from_millis(5000))).await {
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

			match bybit.balances(Instrument::Perp, Some(Duration::from_millis(5000))).await {
				Ok(_) => println!("✅ Bybit: API key is valid and active"),
				Err(e) => println!("❌ Bybit: API key error - {}", e),
			}
		}
		_ => println!("⚠️  Bybit: Environment variables {} or {} not set", key_var, secret_var),
	}
}

async fn check_kucoin() {
	println!("🔍 Checking Kucoin...");

	let key_var = "KUCOIN_API_PUBKEY";
	let secret_var = "KUCOIN_API_SECRET";
	let passphrase_var = "KUCOIN_API_PASSPHRASE";

	match (env::var(key_var), env::var(secret_var), env::var(passphrase_var)) {
		(Ok(key), Ok(secret), Ok(passphrase)) => {
			#[cfg(feature = "kucoin")]
			{
				use v_exchanges_adapters::kucoin::KucoinOption;
				let mut kucoin = ExchangeName::Kucoin.init_client();
				kucoin.update_default_option(KucoinOption::Pubkey(key));
				kucoin.update_default_option(KucoinOption::Secret(secret.into()));
				kucoin.update_default_option(KucoinOption::Passphrase(passphrase.into()));

				match kucoin.balances(Instrument::Spot, None).await {
					Ok(_) => println!("✅ Kucoin: API key is valid and active"),
					Err(e) => {
						let err_str = e.to_string();
						if err_str.contains("400003") || err_str.contains("KC-API-KEY not exists") {
							println!("❌ Kucoin: API key does not exist or has been deleted");
						} else if err_str.contains("400004") || err_str.contains("KC-API-PASSPHRASE") {
							println!("❌ Kucoin: Invalid passphrase");
						} else if err_str.contains("400005") || err_str.contains("Signature") {
							println!("❌ Kucoin: Invalid signature (check API secret)");
						} else if err_str.contains("400006") || err_str.contains("timestamp") {
							println!("❌ Kucoin: Invalid timestamp");
						} else if err_str.contains("400007") || err_str.contains("KC-API-KEY-VERSION") {
							println!("❌ Kucoin: Invalid API key version");
						} else {
							println!("❌ Kucoin: API key error - {}", e);
						}
					}
				}
			}
			#[cfg(not(feature = "kucoin"))]
			{
				println!("⚠️  Kucoin: Feature not enabled (compile with --features kucoin)");
			}
		}
		_ => println!("⚠️  Kucoin: Environment variables {}, {}, or {} not set", key_var, secret_var, passphrase_var),
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

			match mexc.balances(Instrument::Perp, Some(Duration::from_millis(5000))).await {
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
