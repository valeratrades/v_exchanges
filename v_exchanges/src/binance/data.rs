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
use crate::{ExchangeError, ExchangeName, ExchangeResult, core::RequestRange, other_types::Lsrs, utils::join_params};

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
	pub async fn lsr(&self, pair: Pair, tf: Timeframe, range: RequestRange, who: LsrWho) -> Result<Lsrs, ExchangeError> {
		range.ensure_allowed(0..=500, &tf)?;
		let range_json = range.serialize(ExchangeName::Binance);

		let ending = match who {
			LsrWho::Global => "globalLongShortAccountRatio",
			LsrWho::Top => "topLongShortPositionRatio",
		};
		let base_json = json!({
			"symbol": pair.fmt_binance(),
			"period": tf,
		});
		let params = join_params(base_json, range_json);
		let options = [BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM)];
		let r: serde_json::Value = self.get(&format!("/futures/data/{ending}"), &params, options).await?;
		let r: Vec<LsrResponse> = serde_json::from_value(r).unwrap();
		Ok(Lsrs {
			values: r.into_iter().map(LsrResponse::from).collect(),
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
impl From<LsrResponse> for Lsr {
	fn from(r: LsrResponse) -> Self {
		Self {
			time: DateTime::from_timestamp_millis(r.timestamp).unwrap(),
			long: Percent::from_str(&r.long_account).unwrap(),
		}
	}
}
