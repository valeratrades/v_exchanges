use std::collections::BTreeMap;

use adapters::{
	Client,
	binance::{BinanceOption, BinanceWsHandler, BinanceWsUrl},
	generics::ws::{WsConnection, WsError},
};
use jiff::Timestamp;
use v_utils::trades::Pair;

use crate::{BatchTrades, BookShape, BookUpdate, ExchangeStream, Instrument, PrecisionPriceQty, core::InnerTrade};

// trades {{{
#[derive(Debug)]
pub struct TradesConnection {
	connection: WsConnection<BinanceWsHandler>,
	instrument: Instrument,
	pair_precisions: BTreeMap<Pair, PrecisionPriceQty>,
}
impl TradesConnection {
	pub fn try_new(client: &Client, pairs: &[Pair], instrument: Instrument, pair_precisions: BTreeMap<Pair, PrecisionPriceQty>) -> Result<Self, WsError> {
		let vec_topic_str = pairs.iter().map(|p| format!("{}@trade", p.fmt_binance().to_lowercase())).collect::<Vec<_>>();

		let base_url = match instrument {
			Instrument::Perp => BinanceWsUrl::FuturesUsdM,
			Instrument::Spot | Instrument::Margin => BinanceWsUrl::Spot,
			_ => unimplemented!(),
		};
		let connection = client.ws_connection("", vec![BinanceOption::WsUrl(base_url), BinanceOption::WsTopics(vec_topic_str)])?;

		Ok(Self {
			connection,
			instrument,
			pair_precisions,
		})
	}
}
#[async_trait::async_trait]
impl ExchangeStream for TradesConnection {
	type Item = BatchTrades;

	async fn next(&mut self) -> Result<Self::Item, WsError> {
		loop {
			let content_event = self.connection.next().await?;
			let (pair_str, timestamp, qty_asset_str, price_str) = match self.instrument {
				Instrument::Perp => {
					let parsed = serde_json::from_value::<TradeEventPerp>(content_event.data.clone()).expect("Exchange responded with invalid trade event");
					(parsed.pair, parsed.timestamp, parsed.qty_asset, parsed.price)
				}
				Instrument::Spot | Instrument::Margin => {
					let parsed = serde_json::from_value::<TradeEventSpot>(content_event.data.clone()).expect("Exchange responded with invalid trade event");
					(parsed.pair, parsed.timestamp, parsed.qty_asset, parsed.price)
				}
				_ => unimplemented!(),
			};
			let pair: Pair = pair_str.as_str().try_into().unwrap_or_else(|_| panic!("failed to parse pair from trade event: {pair_str}"));
			let prec = *self.pair_precisions.get(&pair).unwrap_or_else(|| panic!("{pair} not in pair_precisions"));

			let price_raw = prec.parse_price(&price_str);
			let qty_raw = prec.parse_qty(&qty_asset_str);
			if (price_raw == 0 || qty_raw == 0) {
				if content_event.data.get("X").unwrap().as_str().unwrap() == "NA" {
					tracing::warn!(
						raw_json = %content_event.data,
						topic = %content_event.topic,
						event_type = %content_event.event_type,
						event_time = %content_event.time,
						"Binance sent a zero-valued trade. Normally it will have X = NA, meaning bookkeeping artifact). But we hit it for something else. I heard X=ADL or X=INSURANCE_FUND could fall here, but not certain. Gotta study if happens..",
					)
				}
				continue;
			}

			let trade = InnerTrade {
				time: Timestamp::from_millisecond(timestamp).expect("Exchange responded with invalid timestamp"),
				price: price_raw,
				qty: qty_raw,
			};
			return Ok(BatchTrades { prec, trades: vec![trade] });
		}
	}
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct TradeEventPerp {
	#[serde(rename = "T")]
	timestamp: i64,
	#[serde(rename = "X")]
	_order_type: String,
	#[serde(rename = "m")]
	_is_maker: bool,
	#[serde(rename = "q")]
	qty_asset: String,
	#[serde(rename = "p")]
	price: String,
	#[serde(rename = "s")]
	pair: String,
	#[serde(rename = "t")]
	_trade_id: u64,
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct TradeEventSpot {
	#[serde(rename = "T")]
	timestamp: i64,
	#[serde(rename = "q")]
	qty_asset: String,
	#[serde(rename = "p")]
	price: String,
	#[serde(rename = "s")]
	pair: String,
}

//,}}}

// book {{{
#[derive(Debug)]
pub struct BookConnection {
	connection: WsConnection<BinanceWsHandler>,
	pair_precisions: BTreeMap<Pair, PrecisionPriceQty>,
}
impl BookConnection {
	pub fn try_new(client: &Client, pairs: &[Pair], instrument: Instrument, pair_precisions: BTreeMap<Pair, PrecisionPriceQty>) -> Result<Self, WsError> {
		let vec_topic_str = pairs.iter().map(|p| format!("{}@depth@100ms", p.fmt_binance().to_lowercase())).collect::<Vec<_>>();

		let base_url = match instrument {
			Instrument::Perp => BinanceWsUrl::FuturesUsdM,
			Instrument::Spot | Instrument::Margin => BinanceWsUrl::Spot,
			_ => unimplemented!(),
		};
		let connection = client.ws_connection("", vec![BinanceOption::WsUrl(base_url), BinanceOption::WsTopics(vec_topic_str)])?;

		Ok(Self { connection, pair_precisions })
	}
}
#[async_trait::async_trait]
impl ExchangeStream for BookConnection {
	type Item = BookUpdate;

	async fn next(&mut self) -> Result<Self::Item, WsError> {
		let content_event = self.connection.next().await?;
		let parsed: DepthEvent = serde_json::from_value(content_event.data.clone()).expect("Exchange responded with invalid depth event");
		let time = parsed
			.transaction_time
			.map(|ts| Timestamp::from_millisecond(ts).expect("Exchange responded with invalid timestamp"))
			.unwrap_or(content_event.time);

		// topic: "btcusdt@depth@100ms" → take before first '@' → uppercase → pair
		let pair_str = content_event.topic.split('@').next().expect("Binance depth topic always contains '@'").to_uppercase();
		let pair: Pair = pair_str
			.as_str()
			.try_into()
			.unwrap_or_else(|_| panic!("failed to parse pair from depth topic: {}", content_event.topic));
		let prec = *self.pair_precisions.get(&pair).unwrap_or_else(|| panic!("{pair} not in pair_precisions"));

		let parse_level = |(p, q): (String, String)| -> (i32, u32) { (prec.parse_price(&p), prec.parse_qty(&q)) };
		let shape = BookShape {
			time,
			prec,
			bids: parsed.bids.into_iter().map(parse_level).collect(),
			asks: parsed.asks.into_iter().map(parse_level).collect(),
		};
		Ok(BookUpdate::BatchDelta(shape))
	}
}

/// Binance diff depth stream event.
/// Spot: https://developers.binance.com/docs/binance-spot-api-docs/web-socket-streams#diff-depth-stream
/// Futures: https://developers.binance.com/docs/derivatives/usds-margined-futures/websocket-market-streams/Diff-Book-Depth-Streams
#[derive(Clone, Debug, serde::Deserialize)]
struct DepthEvent {
	/// Transaction time. Present on futures, absent on spot.
	#[serde(rename = "T")]
	transaction_time: Option<i64>,
	/// Bids: [[price, qty], ...]
	#[serde(rename = "b")]
	bids: Vec<(String, String)>,
	/// Asks: [[price, qty], ...]
	#[serde(rename = "a")]
	asks: Vec<(String, String)>,
}
//,}}}
