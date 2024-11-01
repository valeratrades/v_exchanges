# v_exchanges
![Minimum Supported Rust Version](https://img.shields.io/badge/nightly-1.83+-ab6000.svg)
[<img alt="crates.io" src="https://img.shields.io/crates/v/v_exchanges.svg?color=fc8d62&logo=rust" height="20" style=flat-square>](https://crates.io/crates/v_exchanges)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs&style=flat-square" height="20">](https://docs.rs/v_exchanges)
[<img alt="build status" src="https://img.shields.io/github/actions/workflow/status/valeratrades/v_exchanges/ci.yml?branch=master&style=for-the-badge&style=flat-square" height="20">](https://github.com/valeratrades/v_exchanges/actions?query=branch%3Amaster) <!--NB: Won't find it if repo is private-->
![Lines Of Code](https://img.shields.io/badge/LoC-2032-lightblue)

Trying to make a unified library for all crypto exchange interactions, instead of redefining the response structs again and again.


# Plan 
- [x] Copy over `crypto-botters`
- [x] For binance, copy over the struct definitions from binance-rs
- [ ] implement kline methods, similar to what binance-rs has, but using `crypto-botters` implementation for binance interactions.
- [ ] Now Implement Exchange::klines on Binance.
... Later steps should become apparent after


1. Make a slow-test that would call all the implemented endpoints, then store the responses in the directory with test response payloads. // for signed endpoints just use test-network


<!-- markdownlint-disable -->
<details>
  <summary>
    <h2>Installation</h2>
  </summary>
	<pre><code class="language-sh">TODO</code></pre>
</details>
<!-- markdownlint-restore -->

## Usage
TODO



## Relevant projects
- [crypto-botters](<https://github.com/negi-grass/crypto-botters>), from where I stole the entire `generic-api-client`.
- [binance-rs](<https://github.com/wisespace-io/binance-rs>), which provided a cheat-sheet for so many binance interactions and best-practices on testing.


<br>

<sup>
This repository follows <a href="https://github.com/valeratrades/.github/tree/master/best_practices">my best practices</a>.
</sup>

#### License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
</sub>
