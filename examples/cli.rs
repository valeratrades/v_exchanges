use std::str::FromStr as _;

use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let mut args_iter = std::env::args().skip(1); // eg "binance:BTC-USDT.P"
	let ticker: Ticker = match Ticker::from_str(&args_iter.next().unwrap()) {
		Ok(m) => m,
		Err(e) => {
			eprintln!("Error: {e}");
			std::process::exit(1);
		}
	};
	let client = ticker.exchange_name.init_client();

	let klines: Klines = client.klines(ticker.symbol, "1m".into(), 2.into()).await.unwrap();
	println!("{:#?}", klines.v);

	let prices = client.prices(None, ticker.symbol.instrument).await.unwrap();
	println!("{:?}", &prices.iter().collect::<Vec<_>>()[..5]);
}
