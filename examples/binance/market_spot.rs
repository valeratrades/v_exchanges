use std::str::FromStr as _;

use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let client = ExchangeName::Binance.init_client();
	let symbol = Symbol::from_str("BTC-USDT").unwrap(); // with current impl assumes spot for Instrument (2025/04/27). Equivalent to `Symbol::new(("BTC", "USDT").into(), Instrument::Spot)`

	let spot_klines = client.klines(symbol, "1m".into(), 2.into()).await.unwrap();
	dbg!(&spot_klines);

	let spot_prices = client.prices(None, symbol.instrument).await.unwrap();
	dbg!(&spot_prices.iter().collect::<Vec<_>>()[..5]);
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
