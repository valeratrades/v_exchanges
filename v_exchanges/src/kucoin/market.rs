use std::collections::{BTreeMap, VecDeque};

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges_adapters::kucoin::{KucoinHttpUrl, KucoinOption};
use v_utils::trades::{Kline, Ohlc, Pair};

use crate::{
	ExchangeResult, RequestRange, Symbol,
	core::{ExchangeInfo, Klines, PairInfo},
	kucoin::KucoinTimeframe,
};

// price {{{
pub(super) async fn price(client: &v_exchanges_adapters::Client, pair: Pair, _recv_window: Option<std::time::Duration>) -> ExchangeResult<f64> {
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

// prices {{{
pub(super) async fn prices(client: &v_exchanges_adapters::Client, pairs: Option<Vec<Pair>>, _recv_window: Option<std::time::Duration>) -> ExchangeResult<BTreeMap<Pair, f64>> {
	let options = vec![KucoinOption::HttpUrl(KucoinHttpUrl::Spot)];
	let response: AllTickersResponse = client.get("/api/v1/market/allTickers", &json!({}), options).await?;

	let mut price_map = BTreeMap::new();

	for ticker in response.data.ticker {
		// Parse Kucoin symbol format (e.g., "BTC-USDT" -> Pair)
		if let Some((base, quote)) = ticker.symbol.split_once('-') {
			let pair = Pair::new(base, quote);

			// If pairs filter is specified, only include those pairs
			if let Some(ref requested_pairs) = pairs
				&& !requested_pairs.contains(&pair)
			{
				continue;
			}

			price_map.insert(pair, ticker.last);
		}
	}

	Ok(price_map)
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AllTickersResponse {
	pub code: String,
	pub data: AllTickersData,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AllTickersData {
	pub time: i64,
	pub ticker: Vec<TickerInfo>,
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickerInfo {
	pub symbol: String,
	pub symbol_name: Option<String>,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub buy: Option<f64>,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub sell: Option<f64>,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub change_rate: Option<f64>,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub change_price: Option<f64>,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub high: Option<f64>,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub low: Option<f64>,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub vol: Option<f64>,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub vol_value: Option<f64>,
	#[serde_as(as = "DisplayFromStr")]
	pub last: f64,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub average_price: Option<f64>,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub taker_fee_rate: Option<f64>,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub maker_fee_rate: Option<f64>,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub taker_coef_ficient: Option<f64>,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub maker_coef_ficient: Option<f64>,
}
//,}}}

// klines {{{
pub(super) async fn klines(
	client: &v_exchanges_adapters::Client,
	symbol: Symbol,
	tf: KucoinTimeframe,
	range: RequestRange,
	_recv_window: Option<std::time::Duration>,
) -> ExchangeResult<Klines> {
	let kucoin_symbol = format!("{}-{}", symbol.pair.base(), symbol.pair.quote());

	// Convert from v_utils format (1h, 1d, 1w) to Kucoin API format (1hour, 1day, 1week)
	let tf_str = tf.to_string();
	let type_param = tf_str.replace("m", "min").replace("h", "hour").replace("d", "day").replace("w", "week");

	let mut params = vec![("symbol", kucoin_symbol.as_str()), ("type", type_param.as_str())];

	let (start_at, end_at) = match range {
		RequestRange::Span { since, until } => {
			let start = since.as_second().to_string();
			let end = until.map(|t| t.as_second().to_string()).unwrap_or_else(|| Timestamp::now().as_second().to_string());
			(start, end)
		}
		RequestRange::Limit(_) => {
			// Kucoin doesn't support limit directly, so we'll use a large time range
			let end = Timestamp::now();
			let start = end - tf.duration() * 1500; // Max 1500 candles
			(start.as_second().to_string(), end.as_second().to_string())
		}
	};

	params.push(("startAt", &start_at));
	params.push(("endAt", &end_at));

	let options = vec![KucoinOption::HttpUrl(KucoinHttpUrl::Spot)];
	let response: KlineResponse = client.get("/api/v1/market/candles", &params, options).await?;

	let mut klines_vec = VecDeque::new();

	// Kucoin returns klines in descending order (newest first), so we need to reverse
	for kline_data in response.data.iter().rev() {
		// kline_data format: [time, open, close, high, low, volume, turnover]
		if kline_data.len() >= 7 {
			let timestamp_str = &kline_data[0];
			let timestamp_secs: i64 = timestamp_str.parse().map_err(|e| eyre::eyre!("Failed to parse timestamp: {}", e))?;

			let ohlc = Ohlc {
				open: kline_data[1].parse().map_err(|e| eyre::eyre!("Failed to parse open: {}", e))?,
				high: kline_data[3].parse().map_err(|e| eyre::eyre!("Failed to parse high: {}", e))?,
				low: kline_data[4].parse().map_err(|e| eyre::eyre!("Failed to parse low: {}", e))?,
				close: kline_data[2].parse().map_err(|e| eyre::eyre!("Failed to parse close: {}", e))?,
			};

			klines_vec.push_back(Kline {
				open_time: Timestamp::from_second(timestamp_secs).map_err(|e| eyre::eyre!("Invalid timestamp: {}", e))?,
				ohlc,
				volume_quote: kline_data[6].parse().map_err(|e| eyre::eyre!("Failed to parse turnover: {}", e))?,
				trades: None,
				taker_buy_volume_quote: None,
			});
		}
	}

	Ok(Klines::new(klines_vec, *tf))
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KlineResponse {
	pub code: String,
	pub data: Vec<Vec<String>>,
}
//,}}}

// exchange_info {{{
pub(super) async fn exchange_info(client: &v_exchanges_adapters::Client, _recv_window: Option<std::time::Duration>) -> ExchangeResult<ExchangeInfo> {
	let options = vec![KucoinOption::HttpUrl(KucoinHttpUrl::Spot)];
	let response: SymbolsResponse = client.get("/api/v2/symbols", &json!({}), options).await?;

	let mut pairs = BTreeMap::new();

	for symbol in response.data {
		// Only include enabled trading pairs
		if symbol.enable_trading
			&& let Some((base, quote)) = symbol.symbol.split_once('-')
		{
			let pair = Pair::new(base, quote);
			// Calculate price precision from priceIncrement
			// e.g., 0.0001 -> 4, 0.001 -> 3, 1.0 -> 0
			let price_precision = if symbol.price_increment == 0.0 {
				0
			} else {
				(-symbol.price_increment.log10()).max(0.0).round() as u8
			};
			let pair_info = PairInfo { price_precision };
			pairs.insert(pair, pair_info);
		}
	}

	Ok(ExchangeInfo {
		server_time: Timestamp::now(), // Kucoin doesn't return server time in this endpoint
		pairs,
	})
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SymbolsResponse {
	pub code: String,
	pub data: Vec<KucoinSymbol>,
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KucoinSymbol {
	pub symbol: String,
	pub name: String,
	pub base_currency: String,
	pub quote_currency: String,
	pub fee_currency: String,
	pub market: String,
	#[serde_as(as = "DisplayFromStr")]
	pub base_min_size: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub quote_min_size: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub base_max_size: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub quote_max_size: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub base_increment: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub quote_increment: f64,
	#[serde_as(as = "DisplayFromStr")]
	pub price_increment: f64,
	pub price_limit_rate: Option<String>,
	#[serde_as(as = "Option<DisplayFromStr>")]
	pub min_funds: Option<f64>,
	pub is_margin_enabled: bool,
	pub enable_trading: bool,
}
//,}}}

// ============================================================================
// Futures Market Data
// ============================================================================

pub mod futures {
	use std::collections::{BTreeMap, VecDeque};

	use jiff::Timestamp;
	use serde::{Deserialize, Serialize};
	use serde_json::json;
	use serde_with::{DisplayFromStr, serde_as};
	use v_exchanges_adapters::kucoin::{KucoinHttpUrl, KucoinOption};
	use v_utils::trades::{Kline, Ohlc, Pair};

	use crate::{
		ExchangeResult, RequestRange, Symbol,
		core::{ExchangeInfo, Klines, PairInfo},
		kucoin::KucoinTimeframe,
	};

	/// Kucoin futures uses XBT instead of BTC
	fn to_kucoin_futures_base(base: &str) -> &str {
		match base {
			"BTC" => "XBT",
			other => other,
		}
	}

	/// Convert Kucoin futures base currency back to standard format
	fn from_kucoin_futures_base(base: &str) -> &str {
		match base {
			"XBT" => "BTC",
			other => other,
		}
	}

	// price {{{
	pub(in crate::kucoin) async fn price(client: &v_exchanges_adapters::Client, pair: Pair, _recv_window: Option<std::time::Duration>) -> ExchangeResult<f64> {
		// Kucoin futures symbol format: XBTUSDTM (base + quote + "M" for perpetual)
		let base = to_kucoin_futures_base(pair.base().as_ref());
		let symbol = format!("{base}{}M", pair.quote());
		let params = json!({
			"symbol": symbol,
		});
		let options = vec![KucoinOption::HttpUrl(KucoinHttpUrl::Futures)];
		let response: FuturesTickerResponse = client.get("/api/v1/ticker", &params, options).await?;
		Ok(response.data.price)
	}

	#[derive(Debug, Deserialize, Serialize)]
	#[serde(rename_all = "camelCase")]
	pub struct FuturesTickerResponse {
		pub code: String,
		pub data: FuturesTickerData,
	}

	#[serde_as]
	#[derive(Debug, Deserialize, Serialize)]
	#[serde(rename_all = "camelCase")]
	pub struct FuturesTickerData {
		pub sequence: i64,
		pub symbol: String,
		pub side: String,
		pub size: i64,
		pub trade_id: String,
		#[serde_as(as = "DisplayFromStr")]
		pub price: f64,
		#[serde_as(as = "DisplayFromStr")]
		pub best_bid_price: f64,
		pub best_bid_size: i64,
		#[serde_as(as = "DisplayFromStr")]
		pub best_ask_price: f64,
		pub best_ask_size: i64,
		pub ts: i64,
	}
	//,}}}

	// prices {{{
	pub(in crate::kucoin) async fn prices(client: &v_exchanges_adapters::Client, pairs: Option<Vec<Pair>>, _recv_window: Option<std::time::Duration>) -> ExchangeResult<BTreeMap<Pair, f64>> {
		let options = vec![KucoinOption::HttpUrl(KucoinHttpUrl::Futures)];
		let response: ContractsActiveResponse = client.get("/api/v1/contracts/active", &json!({}), options).await?;

		let mut price_map = BTreeMap::new();

		for contract in response.data {
			// Parse symbol: XBTUSDTM -> BTC-USDT
			let symbol = &contract.symbol;
			if !symbol.ends_with('M') {
				continue;
			}

			// Convert XBT -> BTC
			let base = from_kucoin_futures_base(&contract.base_currency);
			let pair = Pair::new(base, contract.quote_currency.as_str());

			// If pairs filter is specified, only include those pairs
			if let Some(ref requested_pairs) = pairs
				&& !requested_pairs.contains(&pair)
			{
				continue;
			}

			price_map.insert(pair, contract.last_trade_price);
		}

		Ok(price_map)
	}

	#[derive(Debug, Deserialize, Serialize)]
	pub struct ContractsActiveResponse {
		pub code: String,
		pub data: Vec<ContractInfo>,
	}

	#[derive(Debug, Deserialize, Serialize)]
	#[serde(rename_all = "camelCase")]
	pub struct ContractInfo {
		pub symbol: String,
		pub base_currency: String,
		pub quote_currency: String,
		pub settle_currency: String,
		#[serde(rename = "type")]
		pub contract_type: String,
		pub status: String,
		pub multiplier: f64,
		pub tick_size: f64,
		pub lot_size: f64,
		pub max_leverage: i32,
		pub last_trade_price: f64,
	}
	//,}}}

	// klines {{{
	pub(in crate::kucoin) async fn klines(
		client: &v_exchanges_adapters::Client,
		symbol: Symbol,
		tf: KucoinTimeframe,
		range: RequestRange,
		_recv_window: Option<std::time::Duration>,
	) -> ExchangeResult<Klines> {
		// Kucoin futures symbol format: XBTUSDTM
		let base = to_kucoin_futures_base(symbol.pair.base().as_ref());
		let kucoin_symbol = format!("{base}{}M", symbol.pair.quote());

		// granularity is in minutes for futures API
		let granularity = (tf.duration().as_secs() / 60) as u32;

		let (from_ts, to_ts) = match range {
			RequestRange::Span { since, until } => {
				let start = since.as_millisecond();
				let end = until.map(|t| t.as_millisecond()).unwrap_or_else(|| Timestamp::now().as_millisecond());
				(start, end)
			}
			RequestRange::Limit(_) => {
				let end = Timestamp::now();
				let start = end - tf.duration() * 200; // Futures API returns max 200 candles
				(start.as_millisecond(), end.as_millisecond())
			}
		};

		let params = json!({
			"symbol": kucoin_symbol,
			"granularity": granularity,
			"from": from_ts,
			"to": to_ts,
		});

		let options = vec![KucoinOption::HttpUrl(KucoinHttpUrl::Futures)];
		let response: FuturesKlineResponse = client.get("/api/v1/kline/query", &params, options).await?;

		let mut klines_vec = VecDeque::new();

		// Futures klines: [timestamp_ms, open, high, low, close, volume, turnover]
		for kline_data in response.data {
			if kline_data.len() >= 7 {
				let timestamp_ms = kline_data[0] as i64;

				let ohlc = Ohlc {
					open: kline_data[1],
					high: kline_data[2],
					low: kline_data[3],
					close: kline_data[4],
				};

				klines_vec.push_back(Kline {
					open_time: Timestamp::from_millisecond(timestamp_ms).map_err(|e| eyre::eyre!("Invalid timestamp: {}", e))?,
					ohlc,
					volume_quote: kline_data[6],
					trades: None,
					taker_buy_volume_quote: None,
				});
			}
		}

		Ok(Klines::new(klines_vec, *tf))
	}

	#[derive(Debug, Deserialize, Serialize)]
	pub struct FuturesKlineResponse {
		pub code: String,
		pub data: Vec<Vec<f64>>,
	}
	//,}}}

	// exchange_info {{{
	pub(in crate::kucoin) async fn exchange_info(client: &v_exchanges_adapters::Client, _recv_window: Option<std::time::Duration>) -> ExchangeResult<ExchangeInfo> {
		let options = vec![KucoinOption::HttpUrl(KucoinHttpUrl::Futures)];
		let response: ContractsActiveResponse = client.get("/api/v1/contracts/active", &json!({}), options).await?;

		let mut pairs = BTreeMap::new();

		for contract in response.data {
			// Only include active contracts
			if contract.status != "Open" {
				continue;
			}

			// Convert XBT -> BTC
			let base = from_kucoin_futures_base(&contract.base_currency);
			let pair = Pair::new(base, contract.quote_currency.as_str());

			// Calculate price precision from tick_size
			let price_precision = if contract.tick_size == 0.0 {
				0
			} else {
				(-contract.tick_size.log10()).max(0.0).round() as u8
			};

			let pair_info = PairInfo { price_precision };
			pairs.insert(pair, pair_info);
		}

		Ok(ExchangeInfo {
			server_time: Timestamp::now(),
			pairs,
		})
	}
	//,}}}
}
