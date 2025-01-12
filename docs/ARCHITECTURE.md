<!--follows https://matklad.github.io/2021/02/06/ARCHITECTURE.md.html-->
# Architecture
```mermaid
flowchart TD
	S["Standardized layer for interactions"]
	D["Define all endpoints for exchange with it "]
	T["Translation layer to unified Exchange struct and those associated"]

	S --> D
	D --> T
```

### `v_exchanges_api_generics`
Provides a standardized interface for defining how an exchange wants to be communicated with.

### `v_exchanges`

#### `::core`
Defines `Exchange` trait and associated structs. If some interactions/methods can be generalized _on top of_ the specified standard objects, they can also be exposed here (under a feature flag probably).

#### other
Split into sections for each implemented exchange.
In each we make the calls and deserialize the responses from the endpoints we care about, following `Exchange` trait specialization. They are then immediately translated into a general form, which is the same across all exchanges (even if at the cost of some data).

Some functions on specific exchange+endpoint pairs, on `Exchange` trait or not, could build upon other calls (eg stitching together a longer `/aggrTrade` response than what's allowed). When doing so, we still use general forms of return from each established endpoint interaction.

### data endpoints
Some exchanges provide specialized `data` endpoints. Those can't be part of `Exchange` trait, but then I just trivially expose them natively on the associated structs of each exchange, without any traits whatsoever. 
