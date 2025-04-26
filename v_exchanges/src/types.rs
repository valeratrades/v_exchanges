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
		Marg => ".M", //Q: do we care for being able to parse spot/margin diff from ticker defs?
		PerpInverse => ".PERP_INVERSE",
		Options => ".OPTIONS",
	}
}

pub struct Ticker {
	pub pair: Pair,
	pub instrument: Instrument,
	pub exchange_name: ExchangeName,
}
impl std::fmt::Display for Ticker {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}{}{}", self.exchange_name, self.pair, self.instrument)
	}
}

impl std::str::FromStr for Ticker {
	type Err = eyre::Report;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let (exchange_str, rest) = s.split_once(':').ok_or_else(|| eyre::eyre!("Invalid ticker format"))?;
		let exchange_name = ExchangeName::from_str(exchange_str)?;
		let (pair_str, instrument_ticker_str) = rest.split_once('.').map(|(p, i)| (p, format!(".{i}"))).unwrap_or((rest, "".to_owned()));
		let pair = Pair::from_str(pair_str)?;
		let instrument = Instrument::from_str(&instrument_ticker_str)?;

		Ok(Ticker { pair, instrument, exchange_name })
	}
}

mod test {
	#[test]
	fn display() {
		let ticker = super::Ticker {
			pair: super::Pair::new("BTC", "USDT"),
			instrument: super::Instrument::Perp,
			exchange_name: super::ExchangeName::Bybit,
		};
		assert_eq!(ticker.to_string(), "bybit:BTC-USDT.P");
	}

	#[test]
	fn from_str() {
		let ticker_str = "bybit:BTC-USDT.P";
		let ticker: super::Ticker = ticker_str.parse().unwrap();
		assert_eq!(ticker.pair, super::Pair::new("BTC", "USDT"));
		assert_eq!(ticker.instrument, super::Instrument::Perp);
		assert_eq!(ticker.exchange_name, super::ExchangeName::Bybit);
	}
}
