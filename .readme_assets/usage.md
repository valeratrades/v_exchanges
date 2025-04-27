Example evocations of crate's methods are exposed in [./examples], with their `[[example]]` references defined [./v_exchanges/Cargo.toml].
To run:
```sh
cargo run -p v_exchanges --example binance_market_perp
```

The spirit of how the framework is used in code is best described with the following `cli` example:
```rs
use std::str::FromStr as _;

use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	let mut args_iter = std::env::args().skip(1);
	//Ex: "binance:BTC-USDT.P"
	let ticker: Ticker = match Ticker::from_str(&args_iter.next().unwrap()) {
		Ok(m) => m,
		Err(e) => {
			eprintln!("Error: {e}");
			std::process::exit(1);
		}
	};
	let client = ticker.exchange_name.init_client();

	let klines = client.klines(ticker.symbol, "1m".into(), 2.into()).await.unwrap();
	dbg!(&klines);
}
```
if you try the following with different `Exchange`s and `Instruments` encoded into the passed ticker string, you can see that we get same well-defined type, irregardless of quirks and differences of each exchange we're interacting with.
