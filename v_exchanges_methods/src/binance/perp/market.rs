use std::collections::BTreeMap;

use adapters::Client;
//HACK: Methods should be implemented on the central interface struct, following <https://github.com/wisespace-io/binance-rs>.
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges_adapters::binance::{BinanceHttpUrl, BinanceOption};

use crate::{ExchangeResult, prelude::*};

pub async fn prices(client: &Client, pairs: Option<Vec<Pair>>) -> ExchangeResult<BTreeMap<Pair, f64>> {
	let options = vec![BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM)];
	let rs: Vec<PriceObject> = match pairs {
		Some(pairs) => {
			let symbols_json = serde_json::to_string(&pairs.iter().map(|p| p.fmt_binance()).collect::<Vec<_>>()).expect("Vec<String> always serializes");
			let params = json!({ "symbols": symbols_json });
			client.get("/fapi/v1/ticker/price", &params, options).await?
		}
		None => client.get_no_query("/fapi/v2/ticker/price", options).await?,
	};
	Ok(rs
		.into_iter()
		.filter_map(|p| match Pair::from_str(&p.symbol) {
			Ok(pair) => Some((pair, p.price)),
			// this endpoint only returns concatenated symbol strings, so unrepresentable listings (eg `BTCU`) can only be skipped
			Err(e) => {
				tracing::warn!("Skipping unparseable Binance symbol {}: {e}", p.symbol);
				None
			}
		})
		.collect())
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PriceObject {
	#[serde_as(as = "DisplayFromStr")]
	price: f64,
	symbol: String,
	time: i64,
}
