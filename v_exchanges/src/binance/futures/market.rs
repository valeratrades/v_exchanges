//HACK: Methods should be implemented on the central interface struct, following <https://github.com/wisespace-io/binance-rs>.
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges_adapters::binance::{BinanceHttpUrl, BinanceOption};
use v_utils::trades::Pair;

// price {{{
//HACK: not sure this is _the_ thing to use for that (throwing away A LOT of data)
pub async fn price(client: &v_exchanges_adapters::Client, pair: Pair) -> Result<f64> {
	let params = json!({
		"symbol": pair.to_string(),
	});

	let r: MarkPriceResponse = client.get("/fapi/v1/premiumIndex", &params, [BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM)]).await.unwrap();
	let price = r.index_price; // when using this framework, we care for per-exchange price, obviously
	Ok(price)
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

//,}}}
