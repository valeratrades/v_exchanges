use std::str::FromStr;

use secrecy::SecretString;
use v_exchanges::{Exchange, Instrument, Kucoin, Symbol};
use v_exchanges_adapters::kucoin::KucoinOption;
use v_utils::trades::{Asset, Pair};

#[tokio::main]
async fn main() -> eyre::Result<()> {
	let mut kucoin = Kucoin::default();

	// Test public endpoint - get BTC-USDT price
	println!("Testing public endpoint (get price)...");
	let symbol = Symbol {
		pair: Pair::from_str("BTC/USDT")?,
		instrument: Instrument::Spot,
	};
	let price = kucoin.price(symbol, None).await?;
	println!("BTC-USDT price: {}", price);

	// Test authenticated endpoints if credentials are available
	if let (Ok(pubkey), Ok(secret), Ok(passphrase)) = (std::env::var("KUCOIN_API_PUBKEY"), std::env::var("KUCOIN_API_SECRET"), std::env::var("KUCOIN_API_PASSPHRASE")) {
		println!("\nTesting authenticated endpoints...");
		kucoin.update_default_option(KucoinOption::Pubkey(pubkey));
		kucoin.update_default_option(KucoinOption::Secret(SecretString::from(secret)));
		kucoin.update_default_option(KucoinOption::Passphrase(SecretString::from(passphrase)));

		// Test get balances
		println!("Getting account balances...");
		let balances = kucoin.balances(Instrument::Spot, None).await?;
		println!("Total balances: {} (Total USD: {})", balances.len(), balances.total);
		for balance in balances.iter().take(5) {
			println!("  {:?}: {} (USD: {:?})", balance.asset, balance.underlying, balance.usd);
		}

		// Test get specific asset balance
		println!("\nGetting USDT balance...");
		let usdt_balance = kucoin.asset_balance(Asset::new("USDT"), Instrument::Spot, None).await?;
		println!("USDT balance: {} (USD: {:?})", usdt_balance.underlying, usdt_balance.usd);
	} else {
		println!("\nSkipping authenticated tests - credentials not found in environment");
		println!("Set KUCOIN_API_PUBKEY, KUCOIN_API_SECRET, and KUCOIN_API_PASSPHRASE to test authenticated endpoints");
	}

	Ok(())
}
