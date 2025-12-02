use std::env;

use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let mut mexc = Mexc::default();
	let symbol = Symbol::new(("BTC", "USDT").into(), Instrument::Perp);
	mexc.auth(env::var("MEXC_READ_KEY").unwrap(), env::var("MEXC_READ_SECRET").unwrap().into());

	let price = mexc.price(symbol).await.unwrap();
	println!("{price:?}");

	let balances = mexc.balances(symbol.instrument, None).await.unwrap();
	println!("{balances:?}");
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
