use adapters::{
	Client,
	mexc::{MexcHttpUrl, MexcOption},
};
use v_utils::prelude::*;

//TODO: impl spot
pub async fn price(client: &Client, pair: Pair) -> Result<f64> {
	let endpoint = format!("/api/v1/contract/index_price/{}", pair.fmt_mexc());
	let r: PriceResponse = client.get_no_query(&endpoint, [MexcOption::HttpUrl(MexcHttpUrl::Futures)]).await.unwrap();
	Ok(r.data.into())
}

#[allow(unused)]
#[derive(Clone, Debug, Default, derive_new::new, Deserialize)]
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
