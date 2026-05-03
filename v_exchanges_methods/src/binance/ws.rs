use std::{collections::BTreeMap, future::Future, pin::Pin, time::Duration};

use adapters::{
	Client,
	binance::{BinanceOption, BinanceWsHandler, BinanceWsUrl},
	generics::ws::{WsConnection, WsError},
};
use jiff::Timestamp;
use v_utils::trades::Pair;

use crate::{
	BatchTrades, BookShape, BookUpdate, ExchangeError, ExchangeStream, Instrument, PrecisionPriceQty, Symbol,
	core::{BookPersistor, InnerTrade},
};

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
			if price_raw == 0 || qty_raw == 0 {
				if content_event.data.get("X").unwrap().as_str().unwrap() != "NA" {
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
			return Ok(BatchTrades::new(prec, vec![trade]));
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
pub struct BookConnection {
	connection: WsConnection<BinanceWsHandler>,
	symbol_precisions: BTreeMap<Symbol, PrecisionPriceQty>,
	/// Shared by every symbol on this connection (one WS URL per instrument).
	instrument: Instrument,
	// snapshot scheduling
	client: Client,
	symbols: Vec<Symbol>,
	next_symbol_idx: usize,
	/// `freq / symbols.len()`; `None` = disabled.
	per_symbol_interval: Option<Duration>,
	pending_snapshot_fut: Option<Pin<Box<dyn Future<Output = Result<BookShape, ExchangeError>> + Send + Sync>>>,
	/// Tracks which symbol the in-flight snapshot future is fetching, so we can route its result.
	pending_snapshot_symbol: Option<Symbol>,
	persistor: Option<Box<dyn BookPersistor>>,
}
impl BookConnection {
	pub fn try_new(client: Client, symbols: Vec<Symbol>, symbol_precisions: BTreeMap<Symbol, PrecisionPriceQty>, book_snapshot_freq: Option<Duration>) -> Result<Self, WsError> {
		let vec_topic_str = symbols.iter().map(|s| format!("{}@depth@100ms", s.pair.fmt_binance().to_lowercase())).collect::<Vec<_>>();

		// All symbols in a connection must share the same instrument (single WS URL).
		let instrument = symbols.first().map(|s| s.instrument).expect("BookConnection requires at least one symbol");
		assert!(symbols.iter().all(|s| s.instrument == instrument), "BookConnection symbols must share a single instrument");
		let base_url = match instrument {
			Instrument::Perp => BinanceWsUrl::FuturesUsdM,
			Instrument::Spot | Instrument::Margin => BinanceWsUrl::Spot,
			_ => unimplemented!(),
		};
		let connection = client.ws_connection("", vec![BinanceOption::WsUrl(base_url), BinanceOption::WsTopics(vec_topic_str)])?;

		let per_symbol_interval = book_snapshot_freq.map(|f| f / symbols.len() as u32);

		// Seed the initial snapshot future — fires immediately on first next() when enabled.
		let (pending_snapshot_fut, pending_snapshot_symbol): (Pin<Box<dyn Future<Output = Result<BookShape, ExchangeError>> + Send + Sync>>, Option<Symbol>) =
			if let (Some(_), Some(&symbol)) = (per_symbol_interval, symbols.first()) {
				let prec = symbol_precisions[&symbol];
				let client_clone = client.clone();
				let deadline = tokio::time::Instant::now();
				let fut: Pin<Box<dyn Future<Output = Result<BookShape, ExchangeError>> + Send + Sync>> = Box::pin(async move {
					tokio::time::sleep_until(deadline).await;
					crate::binance::market::fetch_book_snapshot(&client_clone, symbol, prec).await
				});
				(fut, Some(symbol))
			} else {
				(Box::pin(std::future::pending()), None)
			};
		// Symbol 0 is claimed by the seed above; next rotation starts at 1.
		let next_symbol_idx = if symbols.len() > 1 { 1 } else { 0 };

		Ok(Self {
			connection,
			symbol_precisions,
			instrument,
			client,
			symbols,
			next_symbol_idx,
			per_symbol_interval,
			pending_snapshot_fut: Some(pending_snapshot_fut),
			pending_snapshot_symbol,
			persistor: None,
		})
	}

	/// Attaches a persistor that captures every snapshot/delta as it flows through `next()`.
	pub fn with_persistor(mut self, persistor: Box<dyn BookPersistor>) -> Self {
		self.persistor = Some(persistor);
		self
	}

	pub fn symbol_precisions(&self) -> &BTreeMap<Symbol, PrecisionPriceQty> {
		&self.symbol_precisions
	}

	pub fn persistor_mut(&mut self) -> Option<&mut (dyn BookPersistor + '_)> {
		self.persistor.as_mut().map(|b| &mut **b as &mut dyn BookPersistor)
	}

	fn build_next_snapshot_fut(&mut self) -> (Pin<Box<dyn Future<Output = Result<BookShape, ExchangeError>> + Send + Sync>>, Option<Symbol>) {
		let Some(interval) = self.per_symbol_interval else {
			return (Box::pin(std::future::pending()), None);
		};
		let symbol = self.symbols[self.next_symbol_idx];
		self.next_symbol_idx = (self.next_symbol_idx + 1) % self.symbols.len();
		let prec = self.symbol_precisions[&symbol];
		let client = self.client.clone();
		let deadline = tokio::time::Instant::now() + interval;
		let fut: Pin<Box<dyn Future<Output = Result<BookShape, ExchangeError>> + Send + Sync>> = Box::pin(async move {
			tokio::time::sleep_until(deadline).await;
			crate::binance::market::fetch_book_snapshot(&client, symbol, prec).await
		});
		(fut, Some(symbol))
	}
}

impl std::fmt::Debug for BookConnection {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("BookConnection")
			.field("connection", &self.connection)
			.field("symbol_precisions", &self.symbol_precisions)
			.field("symbols", &self.symbols)
			.field("next_symbol_idx", &self.next_symbol_idx)
			.field("per_symbol_interval", &self.per_symbol_interval)
			.finish_non_exhaustive()
	}
}

#[async_trait::async_trait]
impl ExchangeStream for BookConnection {
	type Item = BookUpdate;

	async fn next(&mut self) -> Result<Self::Item, WsError> {
		enum Branch {
			Snapshot(Result<BookShape, ExchangeError>),
			Delta(Result<adapters::generics::ws::ContentEvent, WsError>),
		}

		let branch = {
			let Self {
				connection, pending_snapshot_fut, ..
			} = self;
			let pending = pending_snapshot_fut.as_mut().expect("seeded in try_new, replaced on every fire").as_mut();
			tokio::select! {
				biased;
				r = pending => Branch::Snapshot(r),
				r = connection.next() => Branch::Delta(r),
			}
		};

		match branch {
			Branch::Snapshot(r) => {
				let snapshot_pair = self.pending_snapshot_symbol;
				let (next_fut, next_pair) = self.build_next_snapshot_fut();
				self.pending_snapshot_fut = Some(next_fut);
				self.pending_snapshot_symbol = next_pair;
				let shape = r.map_err(|e| WsError::Other(eyre::Report::new(e)))?;
				if let (Some(p), Some(persistor)) = (snapshot_pair, self.persistor.as_deref_mut()) {
					persistor.on_snapshot(p, &shape);
				}
				Ok(BookUpdate::Snapshot(shape))
			}
			Branch::Delta(r) => {
				let content_event = r?;
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
				let symbol = Symbol::new(pair, self.instrument);
				let prec = *self.symbol_precisions.get(&symbol).unwrap_or_else(|| panic!("{symbol:?} not in symbol_precisions"));

				let parse_level = |(p, q): (String, String)| -> (i32, u32) { (prec.parse_price(&p), prec.parse_qty(&q)) };
				let shape = BookShape {
					time,
					prec,
					bids: parsed.bids.into_iter().map(parse_level).collect(),
					asks: parsed.asks.into_iter().map(parse_level).collect(),
				};
				match content_event.event_type.as_str() {
					"depthUpdate" => {
						if let Some(persistor) = self.persistor.as_deref_mut() {
							persistor.on_delta(symbol, &shape);
						}
						Ok(BookUpdate::BatchDelta(shape))
					}
					other => panic!("Binance sent unexpected book event type: {other}"),
				}
			}
		}
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
