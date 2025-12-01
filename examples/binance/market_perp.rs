use std::{env, str::FromStr as _, time::Duration};

use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let mut binance = ExchangeName::Binance.init_client();
	let symbol = Symbol::from_str("BTC-USDT.P").unwrap();
	binance.set_max_tries(3);
	binance.set_timeout(Duration::from_secs(10)); // Increase timeout for large responses like exchange_info

	let exchange_info = binance.exchange_info(symbol.instrument).await.unwrap();
	dbg!(&exchange_info.pairs.iter().take(2).collect::<Vec<_>>());

	let klines = binance.klines(symbol, "1m".into(), 2.into()).await.unwrap();
	let price = binance.price(symbol).await.unwrap();
	let open_interest = binance.open_interest(symbol, "1h".into(), 5.into()).await.unwrap();
	dbg!(&klines, price, &open_interest);

	if let (Ok(key), Ok(secret)) = (env::var("BINANCE_TIGER_READ_PUBKEY"), env::var("BINANCE_TIGER_READ_SECRET")) {
		binance.auth(key, secret.into());
		let balance_usdt = binance.asset_balance("USDT".into(), symbol.instrument, Some(Duration::from_millis(10_000))).await.unwrap();
		dbg!(&balance_usdt);
		let balances = binance.balances(symbol.instrument, Some(Duration::from_millis(10_000))).await.unwrap();
		dbg!(&balances);
	} else {
		eprintln!("BINANCE_TIGER_READ_PUBKEY or BINANCE_TIGER_READ_SECRET is missing, skipping private API methods.");
	}
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
