use adapters::Client;
//HACK: Methods should be implemented on the central interface struct, following <https://github.com/wisespace-io/binance-rs>.
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges_adapters::{
	GetOptions,
	binance::{BinanceHttpUrl, BinanceOption, BinanceOptions},
};
use v_utils::prelude::*;

use crate::{ExchangeResult, recv_window_check};

// price {{{
//HACK: should use /fapi/v2/ticker/price instead
pub async fn price(client: &Client, pair: Pair, recv_window: Option<u16>) -> ExchangeResult<f64> {
	recv_window_check!(recv_window, GetOptions::<BinanceOptions>::default_options(client));
	let params = json!({
		"symbol": pair.fmt_binance(),
	});

	let mut options = vec![BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM)];
	if let Some(rw) = recv_window {
		options.push(BinanceOption::RecvWindow(rw));
	}
	let r: MarkPriceResponse = client.get("/fapi/v1/premiumIndex", &params, options).await?;
	let price = r.index_price; // when using this framework, we care for per-exchange price, obviously
	Ok(price)
}

pub async fn prices(client: &Client, pairs: Option<Vec<Pair>>, recv_window: Option<u16>) -> ExchangeResult<BTreeMap<Pair, f64>> {
	recv_window_check!(recv_window, GetOptions::<BinanceOptions>::default_options(client));
	let mut options = vec![BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM)];
	if let Some(rw) = recv_window {
		options.push(BinanceOption::RecvWindow(rw));
	}
	let rs: Vec<PriceObject> = match pairs {
		Some(pairs) => {
			let params = json!({
				"symbols": pairs.into_iter().map(|p| p.to_string()).collect::<Vec<String>>(),
			});
			client.get("/fapi/v1/ticker/price", &params, options).await?
		}
		None => client.get_no_query("/fapi/v2/ticker/price", options).await?,
	};
	Ok(rs.into_iter().map(Into::into).collect())
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
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
#[derive(Clone, Debug, Deserialize, Serialize)]
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
