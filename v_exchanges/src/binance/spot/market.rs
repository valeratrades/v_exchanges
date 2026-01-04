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
		//TODO!!!: fix this branch
		//BUG: doesn't work for some reason
		Some(pairs) => {
			let params = json!({
				"symbols": pairs.into_iter().map(|p| p.to_string()).collect::<Vec<String>>(),
			});
			dbg!(&params);
			client.get("/api/v3/ticker/price", &params, options).await.unwrap()
		}
		None => client.get_no_query("/api/v3/ticker/price", options).await.unwrap(),
	};

	//let mut prices = Vec::with_capacity(r.0.len());
	let mut prices = BTreeMap::new();
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

pub async fn price(client: &v_exchanges_adapters::Client, pair: Pair) -> ExchangeResult<f64> {
	let params = json!({
		"symbol": pair.fmt_binance(),
	});

	let options = vec![BinanceOption::HttpUrl(BinanceHttpUrl::Spot)];
	let r: AssetPriceResponse = client.get("/api/v3/ticker/price", &params, options).await.unwrap();
	let price = r.price;
	Ok(price)
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
