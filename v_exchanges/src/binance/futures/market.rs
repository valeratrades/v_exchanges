use adapters::Client;
//HACK: Methods should be implemented on the central interface struct, following <https://github.com/wisespace-io/binance-rs>.
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges_adapters::binance::{BinanceHttpUrl, BinanceOption};
use v_utils::prelude::*;

use crate::ExchangeResult;

// price {{{
//HACK: should use /fapi/v2/ticker/price instead
pub async fn price(client: &Client, pair: Pair) -> ExchangeResult<f64> {
	let params = json!({
		"symbol": pair.to_string(),
	});

	let r: MarkPriceResponse = client.get("/fapi/v1/premiumIndex", &params, [BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM)]).await?;
	let price = r.index_price; // when using this framework, we care for per-exchange price, obviously
	Ok(price)
}

pub async fn prices(client: &Client, pairs: Option<Vec<Pair>>) -> ExchangeResult<BTreeMap<Pair, f64>> {
	let rs: Vec<PriceObject> = match pairs {
		Some(pairs) => {
			let params = json!({
				"symbols": pairs.into_iter().map(|p| p.to_string()).collect::<Vec<String>>(),
			});
			client.get("/fapi/v1/ticker/price", &params, [BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM)]).await?
		}
		None => client.get_no_query("/fapi/v2/ticker/price", [BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM)]).await?,
	};
	Ok(rs.into_iter().map(Into::into).collect())
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MarkPriceResponse {
	pub symbol: String,
	#[serde_as(as = "DisplayFromStr")]
	pub mark_price: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub index_price: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub estimated_settle_price: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub last_funding_rate: f64,
	pub next_funding_time: u64,
	pub time: u64,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PriceObject {
	#[serde_as(as = "DisplayFromStr")]
	price: f64,
	symbol: String,
	time: i64,
}
impl From<PriceObject> for (Pair, f64) {
	fn from(p: PriceObject) -> Self {
		(Pair::from_str(&p.symbol).expect("Assume v_utils can handle all Binance pairs"), p.price)
	}
}

//,}}}
