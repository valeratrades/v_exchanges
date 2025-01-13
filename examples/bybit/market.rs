use std::env;

use v_exchanges::{
	bybit::{self, Bybit},
	core::{Exchange, MarketTrait as _},
};

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));

	//let m: Market = "Bybit/Linear".into(); // would be nice to be able to do it like this
	let m = bybit::Market::Linear;
	let mut bb = m.client();

	let klines = bb.klines(("BTC", "USDT").into(), "1m".into(), 2.into(), m).await.unwrap();
	dbg!(&klines);
	let price = bb.price(("BTC", "USDT").into(), m).await.unwrap();
	dbg!(&price);

	if let (Ok(key), Ok(secret)) = (env::var("BYBIT_TIGER_READ_KEY"), env::var("BYBIT_TIGER_READ_SECRET")) {
		bb.auth(key, secret);
		private(&mut bb, m).await;
	} else {
		eprintln!("BYBIT_TIGER_READ_KEY or BYBIT_TIGER_READ_SECRET is missing, skipping private API methods.");
	}
}

async fn private(bb: &mut Bybit, m: bybit::Market) {
	//let key_permissions: serde_json::Value = bb.get_no_query("/v5/user/query-api", [BybitOption::HttpAuth(BybitHttpAuth::V3AndAbove)])
	//	.await
	//	.unwrap();

	let balances = bb.balances(m).await.unwrap();
	dbg!(&balances);
}
