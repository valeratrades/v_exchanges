use std::str::FromStr;

use adapters::binance::{BinanceHttpUrl, BinanceOption};
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_with::{DisplayFromStr, serde_as};
use tracing::instrument;
use v_utils::trades::Pair;

#[instrument(skip_all, fields(?pairs))]
pub async fn prices(client: &v_exchanges_adapters::Client, pairs: Option<Vec<Pair>>) -> Result<Vec<(Pair, f64)>> {
	let r: PricesResponse = match pairs {
		//TODO!!!: fix this branch
		//BUG: doesn't work for some reason
		Some(pairs) => {
			let params = json!({
				"symbols": pairs.into_iter().map(|p| p.to_string()).collect::<Vec<String>>(),
			});
			dbg!(&params);
			client.get("/api/v3/ticker/price", &params, [BinanceOption::HttpUrl(BinanceHttpUrl::Spot)]).await.unwrap()
		}
		None => client.get_no_query("/api/v3/ticker/price", [BinanceOption::HttpUrl(BinanceHttpUrl::Spot)]).await.unwrap(),
	};

	let mut prices = Vec::with_capacity(r.0.len());
	for p in r.0.into_iter() {
		match Pair::from_str(&p.symbol) {
			Ok(pair) => {
				prices.push((pair, p.price));
			}
			Err(e) => {
				tracing::warn!("Failed to parse pair from string: {}", e);
				continue;
			}
		};
	}
	Ok(prices)
}

pub async fn price(client: &v_exchanges_adapters::Client, pair: Pair) -> Result<f64> {
	let params = json!({
		"symbol": pair.to_string(),
	});

	let r: AssetPriceResponse = client.get("/api/v3/ticker/price", &params, [BinanceOption::HttpUrl(BinanceHttpUrl::Spot)]).await.unwrap();
	let price = r.price;
	Ok(price)
}

#[derive(Clone, Debug, Default, derive_new::new, Deserialize, Serialize)]
struct PricesResponse(Vec<AssetPriceResponse>);

#[serde_as]
#[derive(Clone, Debug, Default, derive_new::new, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetPriceResponse {
	symbol: String,
	#[serde_as(as = "DisplayFromStr")]
	price: f64,
}
