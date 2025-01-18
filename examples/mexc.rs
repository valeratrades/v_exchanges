use std::env;

use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let m: AbsMarket = "Mexc/Futures".into();
	let mut c = m.client();
	c.auth(env::var("MEXC_READ_KEY").unwrap(), env::var("MEXC_READ_SECRET").unwrap());

	let balance_usdt = c.asset_balance("USDT".into(), m).await.unwrap();
	dbg!(&balance_usdt);
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
