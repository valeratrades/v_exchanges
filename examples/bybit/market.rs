use std::env;

use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));

	let m: AbsMarket = "Bybit/Linear".into();
	let mut c = m.client();

	let klines = c.klines(("BTC", "USDT").into(), "1m".into(), 2.into(), m).await.unwrap();
	dbg!(&klines);
	let price = c.price(("BTC", "USDT").into(), m).await.unwrap();
	dbg!(&price);

	if let (Ok(key), Ok(secret)) = (env::var("BYBIT_TIGER_READ_KEY"), env::var("BYBIT_TIGER_READ_SECRET")) {
		c.auth(key, secret);
		private(&mut c, m).await;
	} else {
		eprintln!("BYBIT_TIGER_READ_KEY or BYBIT_TIGER_READ_SECRET is missing, skipping private API methods.");
	}
}

async fn private(c: &mut Box<dyn Exchange>, m: AbsMarket) {
	//let key_permissions: serde_json::Value = bb.get_no_query("/v5/user/query-api", [BybitOption::HttpAuth(BybitHttpAuth::V3AndAbove)])
	//	.await
	//	.unwrap();

	let balances = c.balances(m).await.unwrap();
	dbg!(&balances);
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
