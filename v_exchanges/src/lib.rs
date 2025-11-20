#![feature(array_try_map)]
#![feature(formatting_options)]
#![feature(try_blocks)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub extern crate v_exchanges_adapters as adapters;

pub mod core;
pub mod error;
pub(crate) mod other_types;

pub mod prelude {
	pub use std::str::FromStr as _; // it's very annoying to have to manually bring it into the scope every single time. Putting this into preludes of all libraries with any exposed `FromStr` impls at this point.

	#[cfg(feature = "binance")]
	pub use crate::binance::Binance;
	// TODO: bitflyer implementation not yet complete
	// #[cfg(feature = "bitflyer")]
	// pub use crate::bitflyer::Bitflyer;
	#[cfg(feature = "data")]
	pub use crate::bitmex::Bitmex;
	#[cfg(feature = "bybit")]
	pub use crate::bybit::Bybit;
	// TODO: coincheck implementation not yet complete
	// #[cfg(feature = "coincheck")]
	// pub use crate::coincheck::Coincheck;
	#[cfg(feature = "kucoin")]
	pub use crate::kucoin::Kucoin;
	#[cfg(feature = "mexc")]
	pub use crate::mexc::Mexc;
	#[cfg(feature = "data")]
	pub use crate::yahoo::*;
	pub use crate::{core::*, error::*, other_types::*};
}
pub use prelude::*;

pub(crate) mod utils;

#[cfg(feature = "binance")]
#[cfg_attr(docsrs, doc(cfg(feature = "binance")))]
pub mod binance;

#[cfg(feature = "bybit")]
#[cfg_attr(docsrs, doc(cfg(feature = "bybit")))]
pub mod bybit;

#[cfg(feature = "kucoin")]
#[cfg_attr(docsrs, doc(cfg(feature = "kucoin")))]
pub mod kucoin;

#[cfg(feature = "mexc")]
#[cfg_attr(docsrs, doc(cfg(feature = "mexc")))]
pub mod mexc;

cfg_if::cfg_if! {
	if #[cfg(feature = "data")] {
		pub mod bitmex;
		pub mod yahoo;
	}
}
