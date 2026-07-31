use std::{collections::BTreeMap, str::FromStr};

use adapters::binance::{BinanceHttpUrl, BinanceOption};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use serde_with::{DisplayFromStr, serde_as};
use tracing::instrument;
use trading_data_core::{Pair, Precision};

use crate::{
	ExchangeResult,
	core::{ExchangeInfo, PairInfo},
};

#[instrument(skip_all, fields(?pairs))]
pub async fn prices(client: &v_exchanges_adapters::Client, pairs: Option<Vec<Pair>>) -> ExchangeResult<BTreeMap<Pair, f64>> {
	let options = vec![BinanceOption::HttpUrl(BinanceHttpUrl::Spot)];
	let r: PricesResponse = match pairs {
		Some(pairs) => {
			let symbols_json = serde_json::to_string(&pairs.iter().map(|p| p.fmt_binance()).collect::<Vec<_>>()).expect("Vec<String> always serializes");
			let params = json!({ "symbols": symbols_json });
			client.get("/api/v3/ticker/price", &params, options).await?
		}
		None => client.get_no_query("/api/v3/ticker/price", options).await?,
	};

	//let mut prices = Vec::with_capacity(r.0.len());
	let mut prices = BTreeMap::default();
	for p in r.0.into_iter() {
		match Pair::from_str(&p.symbol) {
			Ok(pair) => {
				prices.insert(pair, p.price);
			}
			Err(e) => {
				tracing::warn!("Failed to parse pair from string: {e}");
				continue;
			}
		};
	}
	Ok(prices)
}

pub async fn exchange_info(client: &v_exchanges_adapters::Client) -> ExchangeResult<ExchangeInfo> {
	let options = vec![BinanceOption::HttpUrl(BinanceHttpUrl::Spot)];
	let r: SpotExchangeInfoResponse = client.get_no_query("/api/v3/exchangeInfo", options).await?;
	Ok(r.into())
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, derive_new::new)]
struct PricesResponse(Vec<AssetPriceResponse>);

#[serde_as]
#[derive(Clone, Debug, Default, Deserialize, Serialize, derive_new::new)]
#[serde(rename_all = "camelCase")]
struct AssetPriceResponse {
	symbol: String,
	#[serde_as(as = "DisplayFromStr")]
	price: f64,
}

/// Prices are multiples of the tick by the venue's own filter, so a tick above 1 is a genuinely
/// negative precision — that is what keeps a 12-digit index price inside an i32 raw column.
fn tick_precision(s: &str) -> Precision {
	match s.split_once('.') {
		Some((_, frac)) => Precision(frac.trim_end_matches('0').len() as i8),
		None => Precision(-(s.len() as i8 - s.trim_end_matches('0').len() as i8)),
	}
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpotExchangeInfoResponse {
	server_time: i64,
	symbols: Vec<SpotSymbol>,
}

impl From<SpotExchangeInfoResponse> for ExchangeInfo {
	fn from(r: SpotExchangeInfoResponse) -> Self {
		let pairs = r
			.symbols
			.into_iter()
			.filter(|s| s.status == "TRADING")
			.filter_map(|s| {
				let pair = match Pair::from_str(&s.symbol) {
					Ok(p) => p,
					Err(e) => {
						tracing::warn!("Failed to parse spot pair {}: {e}", s.symbol);
						return None;
					}
				};
				Some((pair, PairInfo::from(s)))
			})
			.collect();
		Self {
			server_time: Timestamp::from_millisecond(r.server_time).expect("Binance serverTime is valid ms"),
			pairs,
		}
	}
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpotSymbol {
	symbol: String,
	status: String,
	base_asset_precision: u8,
	filters: Vec<Value>,
}

impl SpotSymbol {
	fn tick_size(&self) -> Option<&str> {
		self.filters.iter().find_map(|f| if f["filterType"] == "PRICE_FILTER" { f["tickSize"].as_str() } else { None })
	}
}

impl From<SpotSymbol> for PairInfo {
	fn from(s: SpotSymbol) -> Self {
		let price_precision = s.tick_size().map(tick_precision).unwrap_or_default();
		Self {
			price_precision,
			qty_precision: Precision(s.base_asset_precision as i8),
			delivery_date: None,
		}
	}
}
