use std::{env, str::FromStr as _};

use v_exchanges::{Bybit, prelude::*};

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let mut client = Bybit::default();
	let symbol = Symbol::from_str("BTC-USDT.P").unwrap();

	let klines = client.klines(symbol, "1m".into(), 2.into()).await.unwrap();
	println!("{klines:?}");
	let price = client.price(symbol).await.unwrap();
	println!("{price:?}");
	let open_interest = client.open_interest(symbol, "1h".into(), 5.into()).await.unwrap();
	println!("{open_interest:?}");

	let keys_prefix = "QUANTM_BYBIT_SUB";
	let pubkey_name = format!("{keys_prefix}_PUBKEY");
	let secret_name = format!("{keys_prefix}_SECRET");
	if let (Ok(pubkey), Ok(secret)) = (env::var(&pubkey_name), env::var(&secret_name)) {
		client.auth(pubkey, secret.into());
		private(&client, symbol).await;
	} else {
		eprintln!("{pubkey_name} or {secret_name} is missing, skipping private API methods.");
	}
}

async fn private(c: &dyn Exchange, symbol: Symbol) {
	//let key_permissions: serde_json::Value = bb.get_no_query("/v5/user/query-api", [BybitOption::HttpAuth(BybitHttpAuth::V3AndAbove)])
	//	.await
	//	.unwrap();

	let balances = c.balances(None, symbol.instrument).await.unwrap();
	println!("{balances:?}");

	let balance_usdc = c.asset_balance("USDC".into(), Some(5000), symbol.instrument).await.unwrap();
	println!("{balance_usdc:?}");
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
