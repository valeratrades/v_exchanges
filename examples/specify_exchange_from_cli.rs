use std::str::FromStr as _;

use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let mut args_iter = std::env::args().skip(1);
	let m: AbsMarket = match AbsMarket::from_str(&args_iter.next().unwrap()) {
		Ok(m) => m,
		Err(e) => {
			eprintln!("Error: {e}");
			std::process::exit(1);
		}
	};
	let c = m.client();

	let spot_klines = c.klines(("BTC", "USDT").into(), "1m".into(), 2.into(), m).await.unwrap();
	dbg!(&spot_klines);

	let spot_prices = c.prices(None, m).await.unwrap();
	dbg!(&spot_prices.iter().collect::<Vec<_>>()[..5]);
}
