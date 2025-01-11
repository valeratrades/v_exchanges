pub extern crate v_exchanges_adapters as adapters;

pub mod core;

pub mod binance;
pub use binance::Binance;

pub mod bybit;
pub use bybit::Bybit;

pub mod bitmex;
pub use bitmex::Bitmex;
use eyre::Result;

#[derive(Debug, Clone, Copy)]
pub enum Market {
	Binance(binance::Market),
	Bybit(bybit::Market),
	//TODO
}
impl Default for Market {
	fn default() -> Self {
		Self::Binance(binance::Market::default())
	}
}

impl std::fmt::Display for Market {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Market::Binance(m) => write!(f, "Binance/{}", m),
			Market::Bybit(m) => write!(f, "Bybit/{}", m),
		}
	}
}

impl std::str::FromStr for Market {
	type Err = eyre::Error;

	fn from_str(s: &str) -> Result<Self> {
		let parts: Vec<&str> = s.split('/').collect();
		if parts.len() != 2 {
			return Err(eyre::eyre!("Invalid market string: {}", s));
		}
		let market = parts[0];
		let exchange = parts[1];
		match market.to_lowercase().as_str() {
			"binance" => Ok(Self::Binance(exchange.parse()?)),
			"bybit" => Ok(Self::Bybit(exchange.parse()?)),
			_ => Err(eyre::eyre!("Invalid market string: {}", s)),
		}
	}
}
