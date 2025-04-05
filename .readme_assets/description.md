A unified library for all crypto exchange interactions, instead of manually wrapping all methods and keeping track of quirks of different exchanges.
Before having this, I was never able to get production-ready any project relying on more than one exchange.

All methods here are effectively zero-cost. // at the network-interactions scale. There will be some tiny extra allocations here and there for convenience purposes + cost of deserializing
Might later make an additional crate for common wrappers that will not be (eg step-wise collecting ind trades data).
