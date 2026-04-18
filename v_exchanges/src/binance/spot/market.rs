use std::{collections::BTreeMap, str::FromStr};

use adapters::binance::{BinanceHttpUrl, BinanceOption};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_with::{DisplayFromStr, serde_as};
use tracing::instrument;
use v_utils::trades::Pair;

use crate::ExchangeResult;

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
