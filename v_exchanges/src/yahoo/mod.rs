use eyre::{Result, bail, eyre};
use jiff::Timestamp;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde_json::Value;
use v_utils::{NowThen, trades::Close};

pub async fn vix(tf: YahooTimeframe, n: u8) -> Result<Vec<Close>> {
	let mut headers = HeaderMap::new();
	headers.insert(
		USER_AGENT,
		HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36"),
	);

	let client = reqwest::Client::builder().default_headers(headers).build()?;

	let url = "https://query1.finance.yahoo.com/v8/finance/chart/%5EVIX";
	let params = [("interval", tf.to_string()), ("count", format!("{n}"))];

	let response = client.get(url).query(&params).send().await?;

	let data: Value = response.json().await?;
	tracing::debug!("{}", data["chart"]["result"][0]);

	let vix_values = {
		let result = &data["chart"]["result"][0]; // [0] because yahoo allows for batch requests

		let closes = result["indicators"]["quote"][0]/*not sure why this one is wrapped in an array*/["close"]
			.as_array()
			.ok_or_else(|| eyre!("Could not extract VIX closes from Yahoo Finance response"))?;
		let closes: Vec<f64> = closes
			.iter()
			.map(|v| v.as_f64().ok_or_else(|| eyre!("Could not extract VIX close from Yahoo Finance response")))
			.collect::<Result<_, _>>()?;

		let timestamps = result["timestamp"]
			.as_array()
			.ok_or_else(|| eyre!("Could not extract VIX timestamps from Yahoo Finance response"))?;
		let timestamps: Vec<i64> = timestamps
			.iter()
			.map(|v| v.as_i64().ok_or_else(|| eyre!("Could not extract VIX timestamp from Yahoo Finance response")))
			.collect::<Result<_, _>>()?;

		closes
			.into_iter()
			.zip(timestamps.iter())
			.map(|(c, t)| Close {
				close: c,
				timestamp: Timestamp::from_millisecond(*t).unwrap(),
			})
			.collect()
	};

	Ok(vix_values)
}

pub async fn vix_change(tf: YahooTimeframe, n: u8) -> Result<NowThen> {
	let vix_history = vix(tf, n).await?;
	if vix_history.len() < 2 {
		bail!("Not enough VIX data to calculate change, need at least 2 points");
	}
	let now = vix_history.last().unwrap();
	let then = vix_history.first().unwrap();
	Ok(NowThen::new(now.close, then.close).add_duration((now.timestamp - then.timestamp).try_into().unwrap()))
}

crate::define_provider_timeframe!(YahooTimeframe, ["1m", "2m", "5m", "15m", "30m", "60m", "1h", "1d", "5d", "1wk", "1mo"]);
