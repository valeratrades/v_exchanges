use std::{env, str::FromStr as _};

use v_exchanges::{Instrument, Kucoin, prelude::*};
use v_exchanges_adapters::kucoin::KucoinOption;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let mut client = Kucoin::default();
	let symbol = Symbol::from_str("BTC-USDT").unwrap();

	// Test public endpoints (Spot)
	println!("=== Testing Spot Public Endpoints ===\n");

	// Test price
	println!("Testing price()...");
	let price = client.price(symbol).await.unwrap();
	println!("BTC-USDT spot price: ${}\n", price);

	// Test prices (get multiple)
	println!("Testing prices()...");
	let prices = client.prices(None, symbol.instrument).await.unwrap();
	println!("Total spot pairs available: {}", prices.len());
	println!("Sample prices:");
	for (pair, price) in prices.iter().take(5) {
		println!("  {}: ${}", pair, price);
	}
	println!();

	// Test klines
	println!("Testing klines()...");
	let klines = client.klines(symbol, "1h".into(), 5.into()).await.unwrap();
	println!("Retrieved {} klines", klines.len());
	if let Some(first) = klines.front() {
		println!("Latest kline: O:{} H:{} L:{} C:{}", first.ohlc.open, first.ohlc.high, first.ohlc.low, first.ohlc.close);
	}
	println!();

	// Test exchange_info
	println!("Testing exchange_info()...");
	let exchange_info = client.exchange_info(symbol.instrument).await.unwrap();
	println!("Total spot trading pairs: {}", exchange_info.pairs.len());
	if let Some((pair, info)) = exchange_info.pairs.iter().next() {
		println!("Sample pair: {} (precision: {})", pair, info.price_precision);
	}
	println!();

	// Test Futures endpoints
	println!("=== Testing Futures Public Endpoints ===\n");

	let futures_symbol = Symbol::from_str("BTC-USDT.P").unwrap();

	// Test futures price
	println!("Testing futures price()...");
	let futures_price = client.price(futures_symbol).await.unwrap();
	println!("BTC-USDT perp price: ${}\n", futures_price);

	// Test futures prices
	println!("Testing futures prices()...");
	let futures_prices = client.prices(None, Instrument::Perp).await.unwrap();
	println!("Total futures pairs available: {}", futures_prices.len());
	println!("Sample prices:");
	for (pair, price) in futures_prices.iter().take(5) {
		println!("  {}: ${}", pair, price);
	}
	println!();

	// Test futures klines
	println!("Testing futures klines()...");
	let futures_klines = client.klines(futures_symbol, "1h".into(), 5.into()).await.unwrap();
	println!("Retrieved {} futures klines", futures_klines.len());
	if let Some(first) = futures_klines.front() {
		println!("Latest kline: O:{} H:{} L:{} C:{}", first.ohlc.open, first.ohlc.high, first.ohlc.low, first.ohlc.close);
	}
	println!();

	// Test futures exchange_info
	println!("Testing futures exchange_info()...");
	let futures_exchange_info = client.exchange_info(Instrument::Perp).await.unwrap();
	println!("Total futures contracts: {}", futures_exchange_info.pairs.len());
	if let Some((pair, info)) = futures_exchange_info.pairs.iter().next() {
		println!("Sample contract: {} (precision: {})", pair, info.price_precision);
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
