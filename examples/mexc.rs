use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let m: AbsMarket = "Mexc/Futures".into();
	let c = m.client_authenticated("temp".into(), "temp".into());

	let balance_usdt = c.asset_balance("USDT".into(), m).await.unwrap();
	dbg!(&balance_usdt);
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
