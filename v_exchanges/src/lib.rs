#![feature(array_try_map)]
#![feature(formatting_options)]
pub extern crate v_exchanges_adapters as adapters;

pub mod core;

pub mod prelude {
	//pub use crate::core::{AbsMarket, Exchange, MarketTrait as _};
	pub use crate::core::*;
}
pub use prelude::*;

pub(crate) mod utils;

#[cfg(feature = "binance")]
#[cfg_attr(docsrs, doc(cfg(feature = "binance")))]
pub mod binance;

#[cfg(feature = "bybit")]
#[cfg_attr(docsrs, doc(cfg(feature = "bybit")))]
pub mod bybit;

#[cfg(feature = "bitmex")]
#[cfg_attr(docsrs, doc(cfg(feature = "bitmex")))]
pub mod mexc;

#[cfg(feature = "data")]
#[cfg_attr(docsrs, doc(cfg(feature = "data")))]
pub mod bitmex;
