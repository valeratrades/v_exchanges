use chrono::{DateTime, Utc};
use eyre::Result;
use reqwest::Client;
use serde::Deserialize;

#[derive(Clone, Debug, Default)]
pub struct Bitmex {}
impl Bitmex {
	pub async fn bvol(&self, limit: u32) -> Result<Vec<BvolPoint>> {
		let client = Client::new();
		let r = client
			.get(format!("https://www.bitmex.com/api/v1/trade?symbol=.BVOL24H&count={limit}&reverse=true"))
			.send()
			.await?
			.json::<Vec<BvolResponse>>()
			.await?;
		let out = r.into_iter().map(|r| r.into()).collect();
		Ok(out)
	}
}

#[derive(Clone, Debug, Default)]
pub struct BvolPoint {
	pub timestamp: DateTime<Utc>,
	pub price: f64,
}

#[derive(Clone, Debug, Default, derive_new::new, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BvolResponse {
	pub timestamp: DateTime<Utc>,
	pub price: f64,
	pub symbol: String,
	pub side: String,
	pub size: u64,
	pub tick_direction: String,
	pub trd_type: String,
}

impl From<BvolResponse> for BvolPoint {
	fn from(r: BvolResponse) -> Self {
		Self {
			timestamp: r.timestamp,
			price: r.price,
		}
	}
}
