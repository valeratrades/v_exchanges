use std::env;

use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let m: AbsMarket = "Mexc/Futures".into();
	let mut c = m.client();
	c.auth(env::var("MEXC_READ_KEY").unwrap(), env::var("MEXC_READ_SECRET").unwrap().into());

	let price = c.price(("BTC", "USDT").into(), m).await.unwrap();
	dbg!(&price);

	let balances = c.balances(None, m).await.unwrap();
	dbg!(&balances);
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
