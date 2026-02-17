# v_exchanges
![Minimum Supported Rust Version](https://img.shields.io/badge/nightly-1.92+-ab6000.svg)
[<img alt="crates.io" src="https://img.shields.io/crates/v/v_exchanges.svg?color=fc8d62&logo=rust" height="20" style=flat-square>](https://crates.io/crates/v_exchanges)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs&style=flat-square" height="20">](https://docs.rs/v_exchanges)
![Lines Of Code](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/valeratrades/b48e6f02c61942200e7d1e3eeabf9bcb/raw/v_exchanges-loc.json)
<br>
[<img alt="ci errors" src="https://img.shields.io/github/actions/workflow/status/valeratrades/v_exchanges/errors.yml?branch=master&style=for-the-badge&style=flat-square&label=errors&labelColor=420d09" height="20">](https://github.com/valeratrades/v_exchanges/actions?query=branch%3Amaster) <!--NB: Won't find it if repo is private-->
[<img alt="ci warnings" src="https://img.shields.io/github/actions/workflow/status/valeratrades/v_exchanges/warnings.yml?branch=master&style=for-the-badge&style=flat-square&label=warnings&labelColor=d16002" height="20">](https://github.com/valeratrades/v_exchanges/actions?query=branch%3Amaster) <!--NB: Won't find it if repo is private-->

A unified library for all crypto exchange interactions, instead of manually wrapping all methods and keeping track of quirks of different exchanges.
Before having this, I was never able to get production-ready any project relying on more than one exchange.
<!-- markdownlint-disable -->
<details>
<summary>
<h3>Installation</h3>
</summary>

```sh
nix build
```

</details>
<!-- markdownlint-restore -->

## Usage
Example evocations of crate's methods are exposed in [./.readme_assets/examples], with their `[[example]]` references defined [./.readme_assets/v_exchanges/Cargo.toml].
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
		let mut args_iter = std::env::args().skip(1);
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
}
```
if you try the following with different `Exchange`s and `Instruments` encoded into the passed ticker string, you can see that we get same well-defined type, irregardless of quirks and differences of each exchange we're interacting with.

## Roadmap
- [x] full binance integration
    - [x] Copy over `crypto-botters`
    - [x] For binance, copy over the struct definitions from binance-rs
    - [x] distribute the current infrastructure to defined boundaries (add _adapters, keep generic-api-client for now (mb rename to _api_generics later)). Get responses with it.
    - [x] go into src/binance/ on ::, implement klines methods with defined xxxResponse structs, have it just cover the websocket and rest for klines. Print both in main.
        - [x] define core types
        - [x] improve error tracing. If the response fails to deserialize, want to know why. Look up how discretionary_engine does it. Want to print the actual response (+ utils functions to concat when too long (add later)), then the target type.
    - [x] now implement `Exchange` for them (same place for now). Call methods.
    - [x] now implement `Exchange` for bybit.
- [x] full bybit integration
- [\.] polish http interactions in using this API in other projects
- [ ] method to execute _all_ known requests in test mode[^1], on `success`full responses, persist the returned json objects to use in test later.
- [x] use in [btc_line](https://github.com/valeratrades/btc_line) to get Websocket interactions nice and good
- [ ] make fitted for the final stage of full integration into [discretionary_engine](<https://github.com/valeratrades/discretionary_engine>) (requires trade execution/followup methods suite), which would signify production-readiness of this crate.
    upd: really should be just compatible with nautilus-trader; used only for data collection, - _not trading_ 


[^1] where allowed, otherwise use min position size or just skip problematic endpoints

## Relevant projects and documentations
- [crypto-botters](<https://github.com/negi-grass/crypto-botters>), from where I stole the entire `generic-api-client` (as `v_exchanges_api_generics`).
- [binance-rs](<https://github.com/wisespace-io/binance-rs>), which provided a cheat-sheet for so many binance interactions and best-practices on testing.
- [binance-spot-connector-rust](<https://github.com/binance/binance-spot-connector-rust>)


<br>

<sup>
	This repository follows <a href="https://github.com/valeratrades/.github/tree/master/best_practices">my best practices</a> and <a href="https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/TIGER_STYLE.md">Tiger Style</a> (except "proper capitalization for acronyms": (VsrState, not VSRState) and formatting). For project's architecture, see <a href="./docs/ARCHITECTURE.md">ARCHITECTURE.md</a>.
</sup>

#### License

<sup>
	Licensed under <a href="LICENSE">Blue Oak 1.0.0</a>
</sup>

<br>

<sub>
	Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be licensed as above, without any additional terms or conditions.
</sub>

