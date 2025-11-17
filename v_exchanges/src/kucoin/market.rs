use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges_adapters::kucoin::{KucoinHttpUrl, KucoinOption};
use v_utils::trades::Pair;

use crate::ExchangeResult;

// price {{{
pub async fn price(client: &v_exchanges_adapters::Client, pair: Pair, _recv_window: Option<u16>) -> ExchangeResult<f64> {
	let symbol = format!("{}-{}", pair.base(), pair.quote());
	let params = json!({
		"symbol": symbol,
	});
	let options = vec![KucoinOption::HttpUrl(KucoinHttpUrl::Spot)];
	let response: TickerResponse = client.get("/api/v1/market/orderbook/level1", &params, options).await?;
	Ok(response.data.price)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickerResponse {
	pub code: String,
	pub data: TickerData,
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickerData {
	pub time: i64,
	pub sequence: String,
	#[serde_as(as = "DisplayFromStr")]
	pub price: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub size: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub best_bid: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub best_bid_size: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub best_ask: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub best_ask_size: f64,
}
//,}}}
