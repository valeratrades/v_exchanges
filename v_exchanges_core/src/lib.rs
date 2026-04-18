/// Fixed-point quantity. Non-negative. raw = value × 10^precision
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, derive_new::new)]
pub struct Qty {
	pub raw: u64,
	pub precision: u8,
}

/// Fixed-point price. Signed to support spreads and options. raw = value × 10^precision
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, derive_new::new)]
pub struct Price {
	pub raw: i64,
	pub precision: u8,
}

impl Qty {
	pub fn from_f64(value: f64, precision: u8) -> Self {
		let raw = (value * 10f64.powi(precision as i32)).round() as u64;
		Self { raw, precision }
	}

	pub fn as_f64(self) -> f64 {
		self.raw as f64 / 10f64.powi(self.precision as i32)
	}

	pub fn is_zero(self) -> bool {
		self.raw == 0
	}
}

impl Price {
	pub fn from_f64(value: f64, precision: u8) -> Self {
		let raw = (value * 10f64.powi(precision as i32)).round() as i64;
		Self { raw, precision }
	}

	pub fn as_f64(self) -> f64 {
		self.raw as f64 / 10f64.powi(self.precision as i32)
	}

	pub fn is_zero(self) -> bool {
		self.raw == 0
	}

	pub fn max(precision: u8) -> Self {
		Self { raw: i64::MAX, precision }
	}

	pub fn min(precision: u8) -> Self {
		Self { raw: i64::MIN, precision }
	}
}

impl From<Price> for f64 {
	fn from(p: Price) -> f64 {
		p.as_f64()
	}
}

impl From<Qty> for f64 {
	fn from(q: Qty) -> f64 {
		q.as_f64()
	}
}

impl std::str::FromStr for Price {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s.find('.') {
			Some(dot) => {
				let precision = (s.len() - dot - 1) as u8;
				let raw_str = format!("{}{}", &s[..dot], &s[dot + 1..]);
				let raw = raw_str.parse::<i64>().map_err(|e| e.to_string())?;
				Ok(Self { raw, precision })
			}
			None => {
				let raw = s.parse::<i64>().map_err(|e| e.to_string())?;
				Ok(Self { raw, precision: 0 })
			}
		}
	}
}

impl std::str::FromStr for Qty {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s.find('.') {
			Some(dot) => {
				let precision = (s.len() - dot - 1) as u8;
				let raw_str = format!("{}{}", &s[..dot], &s[dot + 1..]);
				let raw = raw_str.parse::<u64>().map_err(|e| e.to_string())?;
				Ok(Self { raw, precision })
			}
			None => {
				let raw = s.parse::<u64>().map_err(|e| e.to_string())?;
				Ok(Self { raw, precision: 0 })
			}
		}
	}
}

impl std::fmt::Display for Price {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:.prec$}", self.as_f64(), prec = self.precision as usize)
	}
}

impl std::fmt::Display for Qty {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:.prec$}", self.as_f64(), prec = self.precision as usize)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn price_round_trip() {
		assert_eq!(Price::from_f64(42000.50, 2).as_f64(), 42000.50);
	}

	#[test]
	fn qty_round_trip() {
		assert_eq!(Qty::from_f64(1.234, 3).as_f64(), 1.234);
	}

	#[test]
	fn qty_is_zero() {
		assert!(Qty::from_f64(0.0, 2).is_zero());
	}

	#[test]
	fn price_integer_addition_exact() {
		// raw integer addition is exact where f64 fails for 0.1 + 0.2
		assert_eq!(Price::from_f64(0.1, 1).raw + Price::from_f64(0.2, 1).raw, Price::from_f64(0.3, 1).raw);
	}

	#[test]
	fn price_from_str() {
		let p: Price = "42000.50".parse().unwrap();
		assert_eq!(p.raw, 4200050);
		assert_eq!(p.precision, 2);
	}

	#[test]
	fn price_from_str_negative() {
		let p: Price = "-1.25".parse().unwrap();
		assert_eq!(p.raw, -125);
		assert_eq!(p.precision, 2);
	}

	#[test]
	fn price_from_str_no_decimal() {
		let p: Price = "100".parse().unwrap();
		assert_eq!(p.raw, 100);
		assert_eq!(p.precision, 0);
	}

	#[test]
	fn qty_from_str() {
		let q: Qty = "1.234".parse().unwrap();
		assert_eq!(q.raw, 1234);
		assert_eq!(q.precision, 3);
	}

	#[test]
	fn qty_from_str_no_decimal() {
		let q: Qty = "50".parse().unwrap();
		assert_eq!(q.raw, 50);
		assert_eq!(q.precision, 0);
	}
}
