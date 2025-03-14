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

#[cfg(feature = "mexc")]
#[cfg_attr(docsrs, doc(cfg(feature = "mexc")))]
pub mod mexc;

macro_rules! data_feature_module {
    ($mod_name:ident) => {
        #[cfg(feature = "data")]
        #[cfg_attr(docsrs, doc(cfg(feature = "data")))]
        pub mod $mod_name;
    };
}
data_feature_module!(bitmex);
data_feature_module!(yahoo);
