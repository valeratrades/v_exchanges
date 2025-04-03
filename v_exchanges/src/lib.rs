#![feature(array_try_map)]
#![feature(doc_auto_cfg)]
#![feature(formatting_options)]
#![feature(try_blocks)]

pub extern crate v_exchanges_adapters as adapters;

pub mod core;

pub mod prelude {
	#[cfg(feature = "binance")]
	pub use crate::binance::Binance;
	#[cfg(feature = "bitflyer")]
	pub use crate::bitflyer::Bitflyer;
	#[cfg(feature = "data")]
	pub use crate::bitmex::Bitmex;
	#[cfg(feature = "bybit")]
	pub use crate::bybit::Bybit;
	#[cfg(feature = "coincheck")]
	pub use crate::coincheck::Coincheck;
	pub use crate::core::*;
	#[cfg(feature = "mexc")]
	pub use crate::mexc::Mexc;
	#[cfg(feature = "data")]
	pub use crate::yahoo::Yahoo;
}
pub use prelude::*;

pub(crate) mod utils;

#[cfg(feature = "binance")]
#[cfg_attr(docsrs, doc(cfg(feature = "binance")))]
pub mod binance;

#[cfg(feature = "bybit")]
#[cfg_attr(docsrs, doc(cfg(feature = "bybit")))]
pub mod bybit;

#[cfg(feature = "mexc")]
#[cfg_attr(docsrs, doc(cfg(feature = "mexc")))]
pub mod mexc;

cfg_if::cfg_if! {
	if #[cfg(feature = "data")] {
		pub mod bitmex;
		pub mod yahoo;
	}
}
