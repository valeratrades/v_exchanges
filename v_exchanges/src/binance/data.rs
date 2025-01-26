use adapters::binance::{BinanceHttpUrl, BinanceOption};
use chrono::{DateTime, Utc};
use derive_more::{Display, FromStr};
use serde::Deserialize;
use serde_json::json;
use v_utils::{
	Percent,
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
			"symbol": pair.to_string(),
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
#[derive(Clone, Debug, Default, Copy)]
pub struct Lsr {
	pub time: DateTime<Utc>,
	pub long: Percent,
}
#[derive(Clone, Debug, Default, derive_more::Deref, derive_more::DerefMut)]
pub struct Lsrs {
	#[deref_mut]
	#[deref]
	pub values: Vec<Lsr>,
	pub pair: Pair,
}
//Q: couldn't decide if `short()` and `long(0` should return `f64` or `Percent`. Postponing the decision.
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
impl From<LsrResponse> for Lsr {
	fn from(r: LsrResponse) -> Self {
		Self {
			time: DateTime::from_timestamp_millis(r.timestamp).unwrap(),
			long: Percent::from_str(&r.long_account).unwrap(),
		}
	}
}
