use eyre::{Result, eyre};
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde_json::Value;

pub async fn vix(tf: YahooTimeframe, n: u8) -> Result<f64> {
	let mut headers = HeaderMap::new();
	headers.insert(
		USER_AGENT,
		HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36"),
	);

	let client = reqwest::Client::builder().default_headers(headers).build()?;

	// Yahoo Finance direct API
	let url = "https://query1.finance.yahoo.com/v8/finance/chart/%5EVIX";
	let params = [("interval", tf.to_string()), ("count", format!("{n}"))];

	let response = client.get(url).query(&params).send().await?;

	let data: Value = response.json().await?;
	dbg!(&data["chart"]["result"]);

	let vix_value = data["chart"]["result"][0]["meta"]["regularMarketPrice"]
		.as_f64()
		.ok_or_else(|| eyre!("Could not extract VIX value from Yahoo Finance response"))?;

	Ok(vix_value)
}

crate::define_provider_timeframe!(YahooTimeframe, ["1m", "2m", "5m", "15m", "30m", "60m", "1h", "1d", "5d", "1wk", "1mo"], "Yahoo");
