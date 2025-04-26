use eyre::bail;
use v_utils::trades::Pair;

//MOVE: to v_utils
macro_rules! define_string_enum {
  ($(#[$meta:meta])* $vis:vis enum $name:ident {
    $($(#[$variant_meta:meta])* $variant:ident => $str:expr),* $(,)?
  }) => {
    $(#[$meta])*
    $vis enum $name {
      $($(#[$variant_meta])* $variant),*
    }

    impl std::fmt::Display for $name {
      fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
          $(Self::$variant => write!(f, "{}", $str)),*
        }
      }
    }

    impl std::str::FromStr for $name {
      type Err = eyre::Report;

      fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
          $($str => Ok(Self::$variant)),*,
          _ => bail!("Invalid {} string: {}", stringify!($name).to_lowercase(), s),
        }
      }
    }
  };
}

define_string_enum! {
	#[derive(Clone, Debug, derive_more::From, PartialEq, Eq)]
	#[non_exhaustive]
	pub enum ExchangeName {
		Binance => "binance",
		Bybit => "bybit",
		Mexc => "mexc",
		BitFlyer => "bitflyer",
		Coincheck => "coincheck",
		Yahoo => "yahook",
	}
}

define_string_enum! {
	#[derive(Clone, Debug, derive_more::From, PartialEq, Eq)]
	#[non_exhaustive]
	pub enum Instrument {
		Spot => "",
		Perp => ".P",
		Margin => ".M", //Q: do we care for being able to parse spot/margin diff from ticker defs?
		PerpInverse => ".PERP_INVERSE",
		Options => ".OPTIONS",
	}
}

pub struct Ticker {
	pub symbol: Symbol,
	pub exchange_name: ExchangeName,
}

impl std::fmt::Display for Ticker {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}:{}", self.exchange_name, self.symbol)
	}
}

impl std::str::FromStr for Ticker {
	type Err = eyre::Report;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let (exchange_str, symbol_str) = s.split_once(':').ok_or_else(|| eyre::eyre!("Invalid ticker format"))?;
		let exchange_name = ExchangeName::from_str(exchange_str)?;
		let symbol = Symbol::from_str(symbol_str)?;

		Ok(Ticker { symbol, exchange_name })
	}
}

pub struct Symbol {
	pub pair: Pair,
	pub instrument: Instrument,
}

impl std::fmt::Display for Symbol {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}{}", self.pair, self.instrument)
	}
}

impl std::str::FromStr for Symbol {
	type Err = eyre::Report;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let (pair_str, instrument_ticker_str) = s.split_once('.').map(|(p, i)| (p, format!(".{i}"))).unwrap_or((s, "".to_owned()));
		let pair = Pair::from_str(pair_str)?;
		let instrument = Instrument::from_str(&instrument_ticker_str)?;

		Ok(Symbol { pair, instrument })
	}
}

mod test {
	#[test]
	fn display() {
		let symbol = super::Symbol {
			pair: super::Pair::new("BTC", "USDT"),
			instrument: super::Instrument::Perp,
		};
		let ticker = super::Ticker {
			symbol,
			exchange_name: super::ExchangeName::Bybit,
		};
		assert_eq!(ticker.to_string(), "bybit:BTC-USDT.P");
	}

	#[test]
	fn from_str() {
		let ticker_str = "bybit:BTC-USDT.P";
		let ticker: super::Ticker = ticker_str.parse().unwrap();
		assert_eq!(ticker.symbol.pair, super::Pair::new("BTC", "USDT"));
		assert_eq!(ticker.symbol.instrument, super::Instrument::Perp);
		assert_eq!(ticker.exchange_name, super::ExchangeName::Bybit);
	}
}
