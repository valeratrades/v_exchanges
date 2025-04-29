use chrono::{DateTime, Utc};
use serde::Deserialize;
use v_utils::{Percent, prelude::*, trades::Pair};

#[derive(Clone, Copy, Debug, Default, derive_more::Deref, derive_more::DerefMut, Deserialize, Serialize)]
pub struct Lsr {
	pub time: DateTime<Utc>,
	#[deref_mut]
	#[deref]
	pub long: Percent,
}
//Q: couldn't decide if `short()` and `long()` should return `f64` or `Percent`. Postponing the decision.
impl Lsr {
	pub fn ratio(&self) -> f64 {
		*self.long / self.short()
	}

	/// Percentage of short positions
	pub fn short(&self) -> f64 {
		1.0 - *self.long
	}

	/// Percentage of long positions. // here only for consistency with `short`
	pub fn long(&self) -> f64 {
		*self.long
	}
}
impl From<f64> for Lsr {
	fn from(f: f64) -> Self {
		Self {
			time: DateTime::default(),
			long: Percent::from(f),
		}
	}
}

#[derive(Clone, Debug, Default, derive_more::Deref, derive_more::DerefMut, Deserialize, Serialize)]
pub struct Lsrs {
	#[deref_mut]
	#[deref]
	pub values: Vec<Lsr>,
	pub pair: Pair,
}
impl Lsrs {
	pub const CHANGE_STR_LEN: usize = 26;
	const MAX_LEN_BASE: usize = 9;

	pub fn values(&self) -> &[Lsr] {
		&self.values
	}

	pub fn last(&self) -> Result<&Lsr> {
		self.values.last().ok_or_else(|| eyre!("Lsrs is empty"))
	}

	fn format_pair(&self) -> String {
		let s = match self.pair.quote().as_ref() {
			"USDT" => self.pair.base().to_string(),
			_ => self.pair.to_string(),
		};
		format!("{:<width$}", s, width = Self::MAX_LEN_BASE)
	}

	pub fn display_short(&self) -> Result<String> {
		Ok(format!("{}: {:.2}", self.format_pair(), self.last()?.long()))
	}

	pub fn display_change(&self) -> Result<String> {
		let diff = NowThen::new(*self.last()?.long, *self.first().expect("can't be empty, otherwise `last()` would have had panicked").long);
		let s = format!("{}: {:<12}", self.format_pair(), diff.to_string()); // `to_string`s are required because rust is dumb as of today and will fuck with padding (2024/01/16)
		Ok(format!("{:<width$}", s, width = Self::CHANGE_STR_LEN))
	}
}

#[cfg(test)]
mod tests {
	use std::sync::OnceLock;
	static INIT: OnceLock<()> = OnceLock::new();
	use super::*;

	fn init() -> (Lsrs, Lsrs) {
		if INIT.get().is_none() {
			let _ = INIT.set(());
			color_eyre::install().unwrap();
		}
		(
			Lsrs {
				values: vec![0.4, 0.5, 0.6, 0.55].into_iter().map(Lsr::from).collect(),
				pair: Pair::from(("BTC", "USDT")),
			},
			Lsrs {
				values: vec![0.9, 0.6, 0.6, 0.7].into_iter().map(Lsr::from).collect(),
				pair: Pair::from(("TRUMP", "SOL")),
			},
		)
	}

	#[test]
	fn display_short_usdt_pair() {
		let lsrs = init();
		insta::assert_snapshot!(lsrs.0.display_short().unwrap(), @"BTC      : 0.55");
	}

	#[test]
	fn display_short_non_usdt_pair() {
		let lsrs = init();
		insta::assert_snapshot!(lsrs.1.display_short().unwrap(), @"TRUMP-SOL: 0.70");
	}

	#[test]
	fn display_change() {
		let lsrs = init();
		insta::assert_snapshot!(lsrs.0.display_change().unwrap(), @"BTC      : 0.55+0.15");
	}

	#[test]
	fn display_change_non_usdt() {
		let lsrs = init();
		insta::assert_snapshot!(lsrs.1.display_change().unwrap(), @"TRUMP-SOL: 0.7-0.2");
	}
}
