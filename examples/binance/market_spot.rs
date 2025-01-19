use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let m: AbsMarket = "Binance/Spot".into();
	let c = m.client();

	let spot_klines = c.klines(("BTC", "USDT").into(), "1m".into(), 2.into(), m).await.unwrap();
	dbg!(&spot_klines);

	let spot_prices = c.prices(None, m).await.unwrap();
	dbg!(&spot_prices.iter().collect::<Vec<_>>()[..5]);
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
