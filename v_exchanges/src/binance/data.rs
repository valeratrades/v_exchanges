use adapters::binance::{BinanceHttpUrl, BinanceOption};
use chrono::{DateTime, Utc};
use derive_more::{Display, FromStr};
use eyre::Result;
use serde::Deserialize;
use serde_json::json;
use v_utils::{
	Percent,
	trades::{Pair, Timeframe},
};

use super::Binance;

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
	pub async fn lsr(&self, pair: Pair, tf: Timeframe, limit: u32, who: LsrWho) -> Result<Vec<Lsr>> {
		let allowed_range = 1..=500;
		//TODO!!: add a `limit outside of range` error, generic for all exchanges
		assert!(allowed_range.contains(&limit));

		let ending = match who {
			LsrWho::Global => "globalLongShortAccountRatio",
			LsrWho::Top => "topLongShortPositionRatio",
		};
		let params = json!({
			"symbol": pair.to_string(),
			"period": tf.format_binance()?,
			"limit": limit,
		});
		let r: serde_json::Value = self
			.0
			.get(&format!("/futures/data/{ending}"), &params, [BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM)])
			.await?;
		let r: Vec<LsrResponse> = serde_json::from_value(r).unwrap();
		Ok(r.into_iter().map(|r| r.into()).collect())
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
#[derive(Clone, Debug, Default, Copy)]
pub struct Lsr {
	pub time: DateTime<Utc>,
	pub pair: Pair,
	pub p_long: Percent,
}
//Q: couldn't decide if `short()` and `long(0` should return `f64` or `Percent`. Postponing the decision.
impl Lsr {
	pub fn ratio(&self) -> f64 {
		*self.p_long / self.short()
	}

	/// Percentage of short positions
	pub fn short(&self) -> f64 {
		1.0 - *self.p_long
	}

	/// Percentage of long positions. // here only for consistency with `short`
	pub fn long(&self) -> f64 {
		*self.p_long
	}
}
impl From<LsrResponse> for Lsr {
	fn from(r: LsrResponse) -> Self {
		Self {
			time: DateTime::from_timestamp_millis(r.timestamp).unwrap(),
			pair: Pair::from_str(&r.symbol).unwrap(),
			p_long: Percent::from_str(&r.long_account).unwrap(),
		}
	}
}
