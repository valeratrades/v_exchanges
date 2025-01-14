pub extern crate v_exchanges_adapters as adapters;

pub mod core;

pub mod prelude {
	//pub use crate::core::{AbsMarket, Exchange, MarketTrait as _};
	pub use crate::core::*;
}
pub use prelude::*;

pub mod utils;

pub mod binance;

pub mod bybit;

pub mod bitmex;
