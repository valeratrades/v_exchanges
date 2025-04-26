use chrono::{DateTime, Utc};
use eyre::{Result, bail};
use reqwest::Client;
use serde::Deserialize;

pub async fn bvol(duration: std::time::Duration) -> Result<Vec<BvolPoint>> {
	let to_cut = duration.as_secs() % (5 * 60);
	let n_5m = (duration.as_secs() - to_cut) / (5 * 60);
	if n_5m == 0 {
		bail!("Provided duration is less than 5m");
	}

	let client = Client::new();
	let r = client
		.get(format!("https://www.bitmex.com/api/v1/trade?symbol=.BVOL24H&count={n_5m}&reverse=true"))
		.send()
		.await?
		.json::<Vec<BvolResponse>>()
		.await?;
	let out = r.into_iter().map(|r| r.into()).collect();
	Ok(out)
}

#[derive(Clone, Debug, Default)]
pub struct Bitmex {}
impl Bitmex {
	/// `duration` will be rounded down to nearest multiple of 5m.
	pub async fn bvol(&self, duration: std::time::Duration) -> Result<Vec<BvolPoint>> {
		bvol(duration).await
	}
}

#[derive(Clone, Debug, Default)]
pub struct BvolPoint {
	pub timestamp: DateTime<Utc>,
	pub price: f64,
}

#[derive(Clone, Debug, Default, Deserialize, derive_new::new)]
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
