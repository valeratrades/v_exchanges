use adapters::{
	Client,
	mexc::{MexcHttpUrl, MexcOption, MexcOptions},
};
use v_exchanges_adapters::GetOptions;
use v_utils::prelude::*;

use crate::{ExchangeResult, recv_window_check};

//TODO: impl spot
pub async fn price(client: &Client, pair: Pair, recv_window: Option<u16>) -> ExchangeResult<f64> {
	recv_window_check!(recv_window, GetOptions::<MexcOptions>::default_options(client));
	let endpoint = format!("/api/v1/contract/index_price/{}", pair.fmt_mexc());
	let mut options = vec![MexcOption::HttpUrl(MexcHttpUrl::Futures)];
	if let Some(rw) = recv_window {
		options.push(MexcOption::RecvWindow(rw));
	}
	let r: PriceResponse = client.get_no_query(&endpoint, options).await.unwrap();
	Ok(r.data.into())
}

#[allow(unused)]
#[derive(Clone, Debug, Default, Deserialize, derive_new::new)]
struct PriceResponse {
	pub code: i32,
	pub data: PriceData,
	pub success: bool,
}

#[allow(unused)]
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PriceData {
	index_price: f64,
	symbol: String,
	timestamp: i64,
}
impl From<PriceData> for f64 {
	fn from(data: PriceData) -> f64 {
		data.index_price
	}
}
