use adapters::binance::{BinanceHttpUrl, BinanceOption};
use chrono::{DateTime, Utc};
use derive_more::{Display, FromStr};
use serde::Deserialize;
use serde_json::json;
use v_utils::{
	Percent,
	prelude::*,
	trades::{Pair, Timeframe},
};

use super::Binance;
use crate::{AbsMarket, ExchangeResult, core::RequestRange, utils::join_params};

#[derive(Clone, Debug, Display, FromStr)]
pub enum LsrWho {
	Global,
	Top,
}
impl From<&str> for LsrWho {
	fn from(s: &str) -> Self {
		Self::from_str(s).unwrap()
	}
}

impl Binance {
	pub async fn lsr(&self, pair: Pair, tf: Timeframe, range: RequestRange, who: LsrWho) -> ExchangeResult<Lsrs> {
		range.ensure_allowed(0..=500, &tf)?;
		let range_json = range.serialize(AbsMarket::Binance(crate::binance::Market::Futures));

		let ending = match who {
			LsrWho::Global => "globalLongShortAccountRatio",
			LsrWho::Top => "topLongShortPositionRatio",
		};
		let base_json = json!({
			"symbol": pair.fmt_binance(),
			"period": tf,
		});
		let params = join_params(base_json, range_json);
		let r: serde_json::Value = self
			.client
			.get(&format!("/futures/data/{ending}"), &params, [BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM)])
			.await?;
		let r: Vec<LsrResponse> = serde_json::from_value(r).unwrap();
		Ok(Lsrs {
			values: r.into_iter().map(|r| r.into()).collect(),
			pair,
		})
	}
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LsrResponse {
	pub symbol: String,
	pub long_account: String,
	pub long_short_ratio: String,
	pub short_account: String,
	pub timestamp: i64,
}
#[derive(Clone, Debug, Default, Copy, derive_more::Deref, derive_more::DerefMut, Deserialize, Serialize)]
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
impl From<LsrResponse> for Lsr {
	fn from(r: LsrResponse) -> Self {
		Self {
			time: DateTime::from_timestamp_millis(r.timestamp).unwrap(),
			long: Percent::from_str(&r.long_account).unwrap(),
		}
	}
}

#[derive(Clone, Debug, Default, derive_more::Deref, derive_more::DerefMut, Deserialize, Serialize)]
pub struct Lsrs {
	#[deref_mut]
	#[deref]
	values: Vec<Lsr>,
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
			"USDT" => format!("{:<width$}", self.pair.base().to_string(), width = Self::MAX_LEN_BASE), // if the quote is NOT usdt, we don't align it (theoretically should help with spotting such)
			_ => self.pair.to_string(),
		};
		format!("{:<width$}", s, width = Self::MAX_LEN_BASE)
	}

	pub fn display_short(&self) -> Result<String> {
		Ok(format!("{}: {:.2}", self.format_pair(), self.last()?.long()))
	}

	pub fn display_change(&self) -> Result<String> {
		let diff = NowThen::new(*self.last()?.long, *self.first().expect("can't be empty, otherwise `last()` would have had panicked").long);
		let diff_f = format!("{:<width$}", diff, width = Self::MAX_LEN_BASE);
		let s = format!("{}: {:<12}", self.format_pair(), diff_f); // `to_string`s are required because rust is dumb as of today (2024/01/16)
		assert_eq!(s.len(), Self::CHANGE_STR_LEN);
		Ok(s)
	}
}

#[cfg(test)]
mod tests {
	use std::sync::OnceLock;
	static INIT: OnceLock<()> = OnceLock::new();
	use super::*;

	fn init() -> Lsrs {
		if INIT.get().is_none() {
			let _ = INIT.set(());
			color_eyre::install().unwrap();
		}
		Lsrs {
			values: vec![0.4, 0.5, 0.6, 0.55].into_iter().map(Lsr::from).collect(),
			pair: Pair::from(("BTC", "USDT")),
		}
	}

	#[test]
	fn display_short() {
		let lsrs = init();
		insta::assert_snapshot!(lsrs.display_short().unwrap(), @"BTC-USDT : 0.55");
	}
}
