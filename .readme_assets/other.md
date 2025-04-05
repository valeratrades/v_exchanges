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
