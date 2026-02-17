use std::collections::{BTreeMap, VecDeque};

use adapters::{
	Client,
	mexc::{MexcHttpUrl, MexcOption},
};
use jiff::Timestamp;
use serde_json::json;
use v_utils::{
	prelude::*,
	trades::{Kline, Ohlc},
};

use crate::{
	ExchangeResult, RequestRange, Symbol,
	core::{ExchangeInfo, Klines, PairInfo},
	mexc::MexcTimeframe,
};

//TODO: impl spot
pub(super) async fn price(client: &Client, pair: Pair) -> ExchangeResult<f64> {
	let endpoint = format!("/api/v1/contract/index_price/{}", pair.fmt_mexc());
	let options = vec![MexcOption::HttpUrl(MexcHttpUrl::Futures)];
	let r: PriceResponse = client.get_no_query(&endpoint, options).await?;
	Ok(r.data.into())
}

#[derive(Clone, Debug, Default, Deserialize)]
struct PriceResponse {
	pub data: PriceData,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PriceData {
	index_price: f64,
}
impl From<PriceData> for f64 {
	fn from(data: PriceData) -> f64 {
		data.index_price
	}
}

// klines {{{
pub(super) async fn klines(client: &Client, symbol: Symbol, tf: MexcTimeframe, range: RequestRange) -> ExchangeResult<Klines> {
	let mexc_symbol = symbol.pair.fmt_mexc();

	// Convert timeframe to Mexc format: 1m -> Min1, 5m -> Min5, 1h -> Min60, 4h -> Hour4, 1d -> Day1
	let tf_str = tf.to_string();
	let interval = match tf_str.as_str() {
		"1m" => "Min1",
		"5m" => "Min5",
		"15m" => "Min15",
		"30m" => "Min30",
		"60m" => "Min60",
		"4h" => "Hour4",
		"1d" => "Day1",
		"1W" => "Week1",
		"1M" => "Month1",
		_ => return Err(eyre::eyre!("Unsupported timeframe: {tf_str}").into()),
	};

	let (start, end) = match range {
		RequestRange::Span { since, until } => {
			let s = since.as_second();
			let e = until.map(|t| t.as_second()).unwrap_or_else(|| Timestamp::now().as_second());
			(s, e)
		}
		RequestRange::Limit(n) => {
			let end = Timestamp::now();
			let start = end - tf.duration() * n as u32;
			(start.as_second(), end.as_second())
		}
	};

	let endpoint = format!("/api/v1/contract/kline/{mexc_symbol}");
	let params = json!({
		"interval": interval,
		"start": start,
		"end": end,
	});
	let options = vec![MexcOption::HttpUrl(MexcHttpUrl::Futures)];
	let response: KlineResponse = client.get(&endpoint, &params, options).await?;

	let mut klines_vec = VecDeque::new();
	let data = response.data;

	// Mexc returns separate arrays for each field
	for i in 0..data.time.len() {
		let ohlc = Ohlc {
			open: data.open[i],
			high: data.high[i],
			low: data.low[i],
			close: data.close[i],
		};

		klines_vec.push_back(Kline {
			open_time: Timestamp::from_second(data.time[i]).map_err(|e| eyre::eyre!("Invalid timestamp: {e}"))?,
			ohlc,
			volume_quote: data.amount[i],
			trades: None,
			taker_buy_volume_quote: None,
		});
	}

	Ok(Klines::new(klines_vec, *tf))
}

#[derive(Debug, Deserialize)]
struct KlineResponse {
	data: KlineData,
}

#[derive(Debug, Deserialize)]
struct KlineData {
	time: Vec<i64>,
	open: Vec<f64>,
	close: Vec<f64>,
	high: Vec<f64>,
	low: Vec<f64>,
	vol: Vec<f64>,
	amount: Vec<f64>,
}
//,}}}

// exchange_info {{{
pub(super) async fn exchange_info(client: &Client) -> ExchangeResult<ExchangeInfo> {
	let options = vec![MexcOption::HttpUrl(MexcHttpUrl::Futures)];
	let response: ContractDetailResponse = client.get_no_query("/api/v1/contract/detail", options).await?;

	let mut pairs = BTreeMap::new();

	for contract in response.data {
		// state 0 = active
		if contract.state != 0 {
			continue;
		}

		let pair = Pair::new(contract.base_coin.as_str(), contract.quote_coin.as_str());

		// priceScale is number of decimal places
		let price_precision = contract.price_scale as u8;

		let pair_info = PairInfo { price_precision };
		pairs.insert(pair, pair_info);
	}

	Ok(ExchangeInfo {
		server_time: Timestamp::now(),
		pairs,
	})
}

#[derive(Debug, Deserialize)]
struct ContractDetailResponse {
	data: Vec<ContractInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContractInfo {
	symbol: String,
	base_coin: String,
	quote_coin: String,
	price_scale: i32,
	state: i32,
}
//,}}}
