use std::{env, str::FromStr as _};

use v_exchanges::{Kucoin, prelude::*};
use v_exchanges_adapters::kucoin::KucoinOption;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let mut client = Kucoin::default();
	let symbol = Symbol::from_str("BTC-USDT").unwrap();

	// Test public endpoints
	println!("=== Testing Public Endpoints ===\n");

	// Test price
	println!("Testing price()...");
	let price = client.price(symbol, None).await.unwrap();
	println!("BTC-USDT price: ${}\n", price);

	// Test prices (get multiple)
	println!("Testing prices()...");
	let prices = client.prices(None, symbol.instrument, None).await.unwrap();
	println!("Total pairs available: {}", prices.len());
	println!("Sample prices:");
	for (pair, price) in prices.iter().take(5) {
		println!("  {}: ${}", pair, price);
	}
	println!();

	// Test klines
	println!("Testing klines()...");
	let klines = client.klines(symbol, "1h".into(), 5.into(), None).await.unwrap();
	println!("Retrieved {} klines", klines.len());
	if let Some(first) = klines.front() {
		println!("Latest kline: O:{} H:{} L:{} C:{}", first.ohlc.open, first.ohlc.high, first.ohlc.low, first.ohlc.close);
	}
	println!();

	// Test exchange_info
	println!("Testing exchange_info()...");
	let exchange_info = client.exchange_info(symbol.instrument, None).await.unwrap();
	println!("Total trading pairs: {}", exchange_info.pairs.len());
	if let Some((pair, info)) = exchange_info.pairs.iter().next() {
		println!("Sample pair: {} (precision: {})", pair, info.price_precision);
	}
	println!();

	// Test authenticated endpoints if credentials are available
	let keys_prefix = "KUCOIN_API";
	let pubkey_name = format!("{keys_prefix}_PUBKEY");
	let secret_name = format!("{keys_prefix}_SECRET");
	let passphrase_name = format!("{keys_prefix}_PASSPHRASE");

	if let (Ok(pubkey), Ok(secret), Ok(passphrase)) = (env::var(&pubkey_name), env::var(&secret_name), env::var(&passphrase_name)) {
		client.update_default_option(KucoinOption::Pubkey(pubkey));
		client.update_default_option(KucoinOption::Secret(secret.into()));
		client.update_default_option(KucoinOption::Passphrase(passphrase.into()));
		private(&client, symbol).await;
	} else {
		eprintln!("{pubkey_name}, {secret_name}, or {passphrase_name} is missing, skipping private API methods.");
	}
}

async fn private(c: &dyn Exchange, symbol: Symbol) {
	println!("=== Testing Private Endpoints ===\n");

	// Test balances
	println!("Testing balances()...");
	let balances = c.balances(symbol.instrument, None).await.unwrap();
	println!("Total balances: {} assets", balances.len());
	println!("Total USD value: ${:.2}", balances.total.0);
	println!("Assets:");
	for balance in balances.iter().take(5) {
		let usd_str = balance.usd.map(|u| format!("${:.2}", u.0)).unwrap_or_else(|| "N/A".to_string());
		println!("  {}: {} (USD: {})", balance.asset, balance.underlying, usd_str);
	}
	println!();

	// Test asset_balance
	println!("Testing asset_balance()...");
	let usdt_balance = c.asset_balance("USDT".into(), symbol.instrument, None).await.unwrap();
	let usd_str = usdt_balance.usd.map(|u| format!("${:.2}", u.0)).unwrap_or_else(|| "N/A".to_string());
	println!("USDT balance: {} (USD: {})", usdt_balance.underlying, usd_str);
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
