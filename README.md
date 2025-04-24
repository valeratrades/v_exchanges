# v_exchanges
![Minimum Supported Rust Version](https://img.shields.io/badge/nightly-1.86+-ab6000.svg)
[<img alt="crates.io" src="https://img.shields.io/crates/v/v_exchanges.svg?color=fc8d62&logo=rust" height="20" style=flat-square>](https://crates.io/crates/v_exchanges)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs&style=flat-square" height="20">](https://docs.rs/v_exchanges)
![Lines Of Code](https://img.shields.io/badge/LoC-5500-lightblue)
<br>
[<img alt="ci errors" src="https://img.shields.io/github/actions/workflow/status/valeratrades/v_exchanges/errors.yml?branch=master&style=for-the-badge&style=flat-square&label=errors&labelColor=420d09" height="20">](https://github.com/valeratrades/v_exchanges/actions?query=branch%3Amaster) <!--NB: Won't find it if repo is private-->
[<img alt="ci warnings" src="https://img.shields.io/github/actions/workflow/status/valeratrades/v_exchanges/warnings.yml?branch=master&style=for-the-badge&style=flat-square&label=warnings&labelColor=d16002" height="20">](https://github.com/valeratrades/v_exchanges/actions?query=branch%3Amaster) <!--NB: Won't find it if repo is private-->

A unified library for all crypto exchange interactions, instead of manually wrapping all methods and keeping track of quirks of different exchanges.
Before having this, I was never able to get production-ready any project relying on more than one exchange.

All methods here are effectively zero-cost. // at the network-interactions scale. There will be some tiny extra allocations here and there for convenience purposes + cost of deserializing
Might later make an additional crate for common wrappers that will not be (eg step-wise collecting ind trades data).
<!-- markdownlint-disable -->
<details>
  <summary>
    <h3>Installation</h3>
  </summary>
<pre><code class="language-sh">nix build</code></pre>
</details>
<!-- markdownlint-restore -->

## Usage
Example evocations of crate's methods are exposed in [./examples], with their `[[example]]` references defined [./v_exchanges/Cargo.toml].
To run:
```sh
cargo run -p v_exchanges --example binance_market
```

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
- [ ] use in [btc_line](https://github.com/valeratrades/btc_line) to get Websocket interactions nice and good
- [ ] make fitted for the final stage of full integration into [discretionary_engine](<https://github.com/valeratrades/discretionary_engine>) (requires trade execution/followup methods suite), which would signify production-readiness of this crate.


[^1] where allowed, otherwise use min position size or just skip problematic endpoints

## Relevant projects and documentations
- [crypto-botters](<https://github.com/negi-grass/crypto-botters>), from where I stole the entire `generic-api-client` (as `v_exchanges_api_generics`).
- [binance-rs](<https://github.com/wisespace-io/binance-rs>), which provided a cheat-sheet for so many binance interactions and best-practices on testing.
- [binance-spot-connector-rust](<https://github.com/binance/binance-spot-connector-rust>)


<br>

<sup>
	This repository follows <a href="https://github.com/valeratrades/.github/tree/master/best_practices">my best practices</a> and <a href="https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/TIGER_STYLE.md">Tiger Style</a> (except "proper capitalization for acronyms": (VsrState, not VSRState) and formatting).
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
