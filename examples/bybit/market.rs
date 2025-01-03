use std::env;

use v_exchanges::{bybit::Bybit, core::Exchange};
use v_exchanges_adapters::bybit::{BybitHttpAuth, BybitHttpUrl, BybitOption};

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));

	let mut bb = Bybit::default();

	if let (Ok(key), Ok(secret)) = (env::var("BYBIT_TIGER_READ_KEY"), env::var("BYBIT_TIGER_READ_SECRET")) {
		bb.update_default_option(BybitOption::Key(key));
		bb.update_default_option(BybitOption::Secret(secret));
		private(&mut bb).await;
	} else {
		eprintln!("BYBIT_TIGER_READ_KEY or BYBIT_TIGER_READ_SECRET is missing, skipping private API methods.");
	}
}

async fn private(bb: &mut Bybit) {
	//let key_permissions: serde_json::Value = bb.get_no_query("/v5/user/query-api", [BybitOption::HttpAuth(BybitHttpAuth::V3AndAbove)])
	//	.await
	//	.unwrap();

	let balances = bb.futures_balances().await.unwrap();
	dbg!(&balances);
}
