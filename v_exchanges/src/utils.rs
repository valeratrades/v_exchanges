use v_utils::prelude::*;

/// # Panics
/// Fine, because given prospected usages, theoretically only developer will see it.
pub fn join_params(a: Value, b: Value) -> Value {
	if let (Value::Object(mut a_map), Value::Object(b_map)) = (a, b) {
		a_map.extend(b_map);
		Value::Object(a_map)
	} else {
		panic!("Both inputs must be JSON objects");
	}
}

pub fn usd_value(underlying: f64, asset: Asset, prices: &BTreeMap<Pair, f64>) -> Result<Usd> {
	if underlying == 0. {
		return Ok(Usd(0.));
	}
	if asset == "USDT" {
		return Ok(Usd(underlying));
	}
	let usdt_pair = Pair::new(asset, "USDT".into());
	let usdt_price = prices.get(&usdt_pair).ok_or_else(|| eyre!("No usdt price found for {asset}, which has non-zero balance."))?;
	Ok((underlying * usdt_price).into())
}

#[macro_export]
macro_rules! define_provider_timeframe {
	($struct_name:ident, $timeframes:expr, $provider_name:expr) => {
		#[derive(Debug, Clone, Default, Copy, derive_more::Deref, derive_more::DerefMut, derive_more::AsRef)]
		pub struct $struct_name(v_utils::trades::Timeframe);

		impl std::fmt::Display for $struct_name {
			fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
				const TIMEFRAMES: [&str; $timeframes.len()] = $timeframes;

				let s = self.0.try_as_predefined(&TIMEFRAMES).expect(concat!(
					"We can't create a ",
					stringify!($struct_name),
					" object if that doesn't succeed in the first place"
				));
				write!(f, "{s}")
			}
		}

		impl TryFrom<v_utils::trades::Timeframe> for $struct_name {
			type Error = $crate::UnsupportedTimeframeError;

			fn try_from(t: v_utils::trades::Timeframe) -> Result<Self, Self::Error> {
				const TIMEFRAMES: [&str; $timeframes.len()] = $timeframes;

				match t.try_as_predefined(&TIMEFRAMES) {
					Some(_) => Ok(Self(t)),
					_ => Err($crate::UnsupportedTimeframeError::new(t, TIMEFRAMES.iter().map(v_utils::trades::Timeframe::from).collect())),
				}
			}
		}
		impl From<&str> for $struct_name {
			fn from(s: &str) -> Self {
				Self(v_utils::trades::Timeframe::from(s))
			}
		}
	};
}
