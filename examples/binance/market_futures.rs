use std::env;

use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let m: AbsMarket = "Binance/Futures".into();
	let mut c = m.client();

	println!("market: {m}");
	println!("source client: {}", c.source_market());

	let exchange_info = c.exchange_info(m).await.unwrap();
	dbg!(&exchange_info.pairs.iter().take(2).collect::<Vec<_>>());

	let klines = c.klines(("BTC", "USDT").into(), "1m".into(), 2.into(), m).await.unwrap();
	let price = c.price(("BTC", "USDT").into(), m).await.unwrap();
	dbg!(&klines, price);

	if let (Ok(key), Ok(secret)) = (env::var("BINANCE_TIGER_READ_KEY"), env::var("BINANCE_TIGER_READ_SECRET")) {
		c.auth(key, secret.into());
		let balance_usdt = c.asset_balance("USDT".into(), m).await.unwrap();
		dbg!(&balance_usdt);
		let balances = c.balances(m).await.unwrap();
		dbg!(&balances);
	} else {
		eprintln!("BINANCE_TIGER_READ_KEY or BINANCE_TIGER_READ_SECRET is missing, skipping private API methods.");
	}
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
