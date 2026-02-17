use std::collections::VecDeque;

use eyre::Result;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges_adapters::binance::{BinanceHttpUrl, BinanceOption};
use v_utils::trades::{Kline, Ohlc};

use super::BinanceTimeframe;
use crate::{
	ExchangeError, ExchangeName, Instrument, Symbol,
	core::{Klines, OpenInterest, RequestRange},
	utils::join_params,
};

// klines {{{
/** # Ex: ```json
[1731448080000,\"88591.90\",\"88630.90\",\"88560.00\",\"88574.10\",\"173.581\",1731448139999,\"15378315.48720\",2800,\"113.654\",\"10069629.84420\",\"0\"]
```
**/
#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct KlineResponse {
	pub open_time: i64,
	#[serde_as(as = "DisplayFromStr")]
	pub open: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub close: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub high: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub low: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub volume: f64,
	/// As of today (2025/01/03), means **NOTHING**, as they will still send what it _SHOULD_ be even if the kline is not yet finished. (fuck you, binance)
	__close_time: i64,
	#[serde_as(as = "DisplayFromStr")]
	pub quote_asset_volume: f64,
	pub number_of_trades: usize,
	#[serde_as(as = "DisplayFromStr")]
	pub taker_buy_base_asset_volume: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub taker_buy_quote_asset_volume: f64,

	__ignore: Option<Value>,
}
#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenInterestResponse {
	#[serde_as(as = "DisplayFromStr")]
	pub symbol: String,
	#[serde_as(as = "DisplayFromStr")]
	pub sum_open_interest: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub sum_open_interest_value: f64,
	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "CMCCirculatingSupply")]
	pub cmc_circulating_supply: f64,
	pub timestamp: i64,
}
pub(super) async fn klines(client: &v_exchanges_adapters::Client, symbol: Symbol, tf: BinanceTimeframe, range: RequestRange) -> Result<Klines, ExchangeError> {
	//TODO: test if embedding params into the url works more consistently (comp number of pairs axum-site is ablle ot get)
	range.ensure_allowed(1..=1000, tf.as_ref())?;
	let range_params = range.serialize(ExchangeName::Binance);
	let base_params = json!({
		"symbol": symbol.pair.fmt_binance(),
		"interval": tf.to_string(),
	});
	let params = join_params(base_params, range_params);

	let (endpoint_prefix, base_url) = match symbol.instrument {
		Instrument::Spot => ("/api/v3", BinanceHttpUrl::Spot),
		Instrument::Perp => ("/fapi/v1", BinanceHttpUrl::FuturesUsdM),
		Instrument::Margin => todo!(),
		_ => unimplemented!(),
	};

	let options = vec![BinanceOption::HttpUrl(base_url)];
	let kline_responses: Vec<KlineResponse> = client.get(&format!("{endpoint_prefix}/klines"), &params, options).await?;

	let r_len = kline_responses.len();
	let mut klines = VecDeque::with_capacity(r_len);
	for (i, k) in kline_responses.into_iter().enumerate() {
		//HACK: have to check against current time instead, because binance returns some dumb shit instead of actual close. Here structured this way in case they fix it in the future.
		let close_time = Timestamp::now().as_millisecond();
		match close_time > k.open_time + (0.99 * tf.duration().as_millis() as f64) as i64 {
			true => {
				let ohlc = Ohlc {
					open: k.open,
					high: k.high,
					low: k.low,
					close: k.close,
				};
				klines.push_back(Kline {
					open_time: Timestamp::from_millisecond(k.open_time).unwrap(),
					ohlc,
					volume_quote: k.quote_asset_volume,
					trades: Some(k.number_of_trades),
					taker_buy_volume_quote: Some(k.taker_buy_quote_asset_volume),
				});
			}
			false => match i == r_len - 1 {
				true => tracing::trace!("Skipped last kline in binance request, as it's incomplete (expected behavior)"),
				false => tracing::warn!("Skipped a kline in binance request, as it's incomplete"),
			},
		}
	}
	Ok(Klines::new(klines, *tf))
}

//,}}}

// open_interest {{{
pub(super) async fn open_interest(client: &v_exchanges_adapters::Client, symbol: Symbol, tf: BinanceTimeframe, range: RequestRange) -> Result<Vec<OpenInterest>, ExchangeError> {
	range.ensure_allowed(1..=500, tf.as_ref())?;
	let range_params = range.serialize(ExchangeName::Binance);
	let base_params = json!({
		"symbol": symbol.pair.fmt_binance(),
		"period": tf.to_string(),
	});
	let params = join_params(base_params, range_params);

	let (endpoint, base_url) = match symbol.instrument {
		Instrument::Perp => ("/futures/data/openInterestHist", BinanceHttpUrl::FuturesUsdM),
		_ =>
			return Err(ExchangeError::Method(crate::MethodError::MethodNotSupported {
				exchange: ExchangeName::Binance,
				instrument: symbol.instrument,
			})),
	};

	let options = vec![BinanceOption::HttpUrl(base_url)];
	let responses: Vec<OpenInterestResponse> = client.get(endpoint, &params, options).await?;

	if responses.is_empty() {
		return Err(ExchangeError::Other(eyre::eyre!("No open interest data returned")));
	}

	// Convert all responses to OpenInterest
	let result: Vec<OpenInterest> = responses
		.into_iter()
		.map(|r| OpenInterest {
			val_asset: r.sum_open_interest,
			val_quote: Some(r.sum_open_interest_value),
			marketcap: Some(r.cmc_circulating_supply),
			timestamp: Timestamp::from_millisecond(r.timestamp).unwrap(),
		})
		.collect();

	Ok(result)
}

//,}}}

#[cfg(test)]
mod tests {
	#[test]
	fn klines() {
		let raw_str = "[1731448080000,\"88591.90\",\"88630.90\",\"88560.00\",\"88574.10\",\"173.581\",1731448139999,\"15378315.48720\",2800,\"113.654\",\"10069629.84420\",\"0\"]";
		let _: super::KlineResponse = serde_json::from_str(raw_str).unwrap();
	}
}
