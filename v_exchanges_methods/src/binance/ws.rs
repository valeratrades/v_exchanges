use std::{collections::BTreeMap, future::Future, pin::Pin, time::Duration};

use adapters::{
	Client,
	binance::{BinanceOption, BinanceWsHandler, BinanceWsUrl},
	generics::ws::{WsConnection, WsError},
};
use jiff::Timestamp;
use v_utils::trades::Pair;

use crate::{
	BatchTrades, BookShape, BookUpdate, ExchangeError, ExchangeStream, Instrument, PrecisionPriceQty,
	core::{BookPersistor, InnerTrade, Sequence},
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
			let now = Timestamp::now();
			return Ok(BatchTrades::new(prec, vec![trade], now, now));
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
	pair_precisions: BTreeMap<Pair, PrecisionPriceQty>,
	/// Shared by every pair on this connection (one WS URL per instrument).
	instrument: Instrument,
	// snapshot scheduling
	client: Client,
	pairs: Vec<Pair>,
	next_pair_idx: usize,
	/// `freq / pairs.len()`; `None` = disabled.
	per_pair_interval: Option<Duration>,
	pending_snapshot_fut: Option<Pin<Box<dyn Future<Output = Result<BookShape, ExchangeError>> + Send + Sync>>>,
	/// Tracks which pair the in-flight snapshot future is fetching, so we can route its result.
	pending_snapshot_pair: Option<Pair>,
	persistor: Option<Box<dyn BookPersistor>>,
	/// Last delta sequence value seen per pair. Used to compute `gapped` on each subsequent delta.
	/// REST snapshots are independent anchors and do not seed/clear this map.
	last_seq: BTreeMap<Pair, BinanceDepthSeq>,
}
impl BookConnection {
	pub fn try_new(
		client: Client,
		pairs: Vec<Pair>,
		instrument: Instrument,
		pair_precisions: BTreeMap<Pair, PrecisionPriceQty>,
		book_snapshot_freq: Option<Duration>,
	) -> Result<Self, WsError> {
		assert!(!pairs.is_empty(), "BookConnection requires at least one pair");
		let vec_topic_str = pairs.iter().map(|p| format!("{}@depth@100ms", p.fmt_binance().to_lowercase())).collect::<Vec<_>>();

		let base_url = match instrument {
			Instrument::Perp => BinanceWsUrl::FuturesUsdM,
			Instrument::Spot | Instrument::Margin => BinanceWsUrl::Spot,
			_ => unimplemented!(),
		};
		let connection = client.ws_connection("", vec![BinanceOption::WsUrl(base_url), BinanceOption::WsTopics(vec_topic_str)])?;

		let per_pair_interval = book_snapshot_freq.map(|f| f / pairs.len() as u32);

		// Seed the initial snapshot future — fires immediately on first next() when enabled.
		let (pending_snapshot_fut, pending_snapshot_pair): (Pin<Box<dyn Future<Output = Result<BookShape, ExchangeError>> + Send + Sync>>, Option<Pair>) =
			if let (Some(_), Some(&pair)) = (per_pair_interval, pairs.first()) {
				let prec = pair_precisions[&pair];
				let client_clone = client.clone();
				let deadline = tokio::time::Instant::now();
				let fut: Pin<Box<dyn Future<Output = Result<BookShape, ExchangeError>> + Send + Sync>> = Box::pin(async move {
					tokio::time::sleep_until(deadline).await;
					crate::binance::market::fetch_book_snapshot(&client_clone, pair, instrument, prec).await
				});
				(fut, Some(pair))
			} else {
				(Box::pin(std::future::pending()), None)
			};
		// Pair 0 is claimed by the seed above; next rotation starts at 1.
		let next_pair_idx = if pairs.len() > 1 { 1 } else { 0 };

		Ok(Self {
			connection,
			pair_precisions,
			instrument,
			client,
			pairs,
			next_pair_idx,
			per_pair_interval,
			pending_snapshot_fut: Some(pending_snapshot_fut),
			pending_snapshot_pair,
			persistor: None,
			last_seq: BTreeMap::new(),
		})
	}

	/// Attaches a persistor that captures every snapshot/delta as it flows through `next()`.
	pub fn with_persistor(mut self, persistor: Box<dyn BookPersistor>) -> Self {
		self.persistor = Some(persistor);
		self
	}

	pub fn pair_precisions(&self) -> &BTreeMap<Pair, PrecisionPriceQty> {
		&self.pair_precisions
	}

	pub fn persistor_mut(&mut self) -> Option<&mut (dyn BookPersistor + '_)> {
		self.persistor.as_mut().map(|b| &mut **b as &mut dyn BookPersistor)
	}

	fn build_next_snapshot_fut(&mut self) -> (Pin<Box<dyn Future<Output = Result<BookShape, ExchangeError>> + Send + Sync>>, Option<Pair>) {
		let Some(interval) = self.per_pair_interval else {
			return (Box::pin(std::future::pending()), None);
		};
		let pair = self.pairs[self.next_pair_idx];
		self.next_pair_idx = (self.next_pair_idx + 1) % self.pairs.len();
		let prec = self.pair_precisions[&pair];
		let client = self.client.clone();
		let instrument = self.instrument;
		let deadline = tokio::time::Instant::now() + interval;
		let fut: Pin<Box<dyn Future<Output = Result<BookShape, ExchangeError>> + Send + Sync>> = Box::pin(async move {
			tokio::time::sleep_until(deadline).await;
			crate::binance::market::fetch_book_snapshot(&client, pair, instrument, prec).await
		});
		(fut, Some(pair))
	}
}

impl std::fmt::Debug for BookConnection {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("BookConnection")
			.field("connection", &self.connection)
			.field("pair_precisions", &self.pair_precisions)
			.field("pairs", &self.pairs)
			.field("next_pair_idx", &self.next_pair_idx)
			.field("per_pair_interval", &self.per_pair_interval)
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
				let snapshot_pair = self.pending_snapshot_pair;
				let (next_fut, next_pair) = self.build_next_snapshot_fut();
				self.pending_snapshot_fut = Some(next_fut);
				self.pending_snapshot_pair = next_pair;
				let shape = r.map_err(|e| WsError::Other(eyre::Report::new(e)))?;
				if let (Some(p), Some(persistor)) = (snapshot_pair, self.persistor.as_deref_mut()) {
					persistor.on_snapshot(p, &shape);
				}
				Ok(BookUpdate::Snapshot(shape))
			}
			Branch::Delta(r) => {
				let content_event = r?;
				let parsed: DepthEvent = serde_json::from_value(content_event.data.clone()).expect("Exchange responded with invalid depth event");
				let ts_event = parsed
					.transaction_time
					.map(|ts| Timestamp::from_millisecond(ts).expect("Exchange responded with invalid timestamp"))
					.unwrap_or(content_event.time);
				let now = Timestamp::now();

				// topic: "btcusdt@depth@100ms" → take before first '@' → uppercase → pair
				let pair_str = content_event.topic.split('@').next().expect("Binance depth topic always contains '@'").to_uppercase();
				let pair: Pair = pair_str
					.as_str()
					.try_into()
					.unwrap_or_else(|_| panic!("failed to parse pair from depth topic: {}", content_event.topic));
				let prec = *self.pair_precisions.get(&pair).unwrap_or_else(|| panic!("{pair} not in pair_precisions"));

				let parse_level = |(p, q): (String, String)| -> (i32, u32) { (prec.parse_price(&p), prec.parse_qty(&q)) };
				let shape = BookShape {
					ts_event,
					ts_init: now,
					ts_last: now,
					prec,
					bids: parsed.bids.into_iter().map(parse_level).collect(),
					asks: parsed.asks.into_iter().map(parse_level).collect(),
				};
				match content_event.event_type.as_str() {
					"depthUpdate" => {
						let seq = match self.instrument {
							Instrument::Perp => BinanceDepthSeq::Perp(BinancePerpSeq {
								u_first: parsed.first_update_id,
								u_final: parsed.final_update_id,
								pu: parsed.previous_final_update_id.expect("Binance perp depth event missing `pu`"),
							}),
							Instrument::Spot | Instrument::Margin => BinanceDepthSeq::Spot(BinanceSpotSeq {
								u_first: parsed.first_update_id,
								u_final: parsed.final_update_id,
							}),
							_ => unimplemented!(),
						};
						let gapped = self.last_seq.get(&pair).map(|prev| seq.has_gap_from_prev(prev)).unwrap_or(false);
						self.last_seq.insert(pair, seq);
						if let Some(persistor) = self.persistor.as_deref_mut() {
							persistor.on_delta(pair, &shape, gapped);
						}
						Ok(BookUpdate::BatchDelta(shape))
					}
					other => panic!("Binance sent unexpected book event type: {other}"),
				}
			}
		}
	}
}

/// Sequence token for Binance spot diff-depth events.
#[derive(Clone, Copy, Debug)]
pub struct BinanceSpotSeq {
	pub u_first: u64,
	pub u_final: u64,
}
/// Sequence token for Binance USD-M perp diff-depth events.
#[derive(Clone, Copy, Debug)]
pub struct BinancePerpSeq {
	pub u_first: u64,
	pub u_final: u64,
	pub pu: u64,
}
/// Single map type covering both spot and perp variants — one connection, one instrument, so all
/// entries on a given connection are the same variant. Mismatch is a logic bug and panics.
#[derive(Clone, Copy, Debug)]
pub enum BinanceDepthSeq {
	Spot(BinanceSpotSeq),
	Perp(BinancePerpSeq),
}
/// Binance diff depth stream event.
/// Spot: https://developers.binance.com/docs/binance-spot-api-docs/web-socket-streams#diff-depth-stream
/// Futures: https://developers.binance.com/docs/derivatives/usds-margined-futures/websocket-market-streams/Diff-Book-Depth-Streams
#[derive(Clone, Debug, serde::Deserialize)]
struct DepthEvent {
	/// Transaction time. Present on futures, absent on spot.
	#[serde(rename = "T")]
	transaction_time: Option<i64>,
	/// First update id in this event.
	#[serde(rename = "U")]
	first_update_id: u64,
	/// Final update id in this event.
	#[serde(rename = "u")]
	final_update_id: u64,
	/// Final update id in the *previous* stream event. Perp-only.
	#[serde(rename = "pu")]
	previous_final_update_id: Option<u64>,
	/// Bids: [[price, qty], ...]
	#[serde(rename = "b")]
	bids: Vec<(String, String)>,
	/// Asks: [[price, qty], ...]
	#[serde(rename = "a")]
	asks: Vec<(String, String)>,
}

impl Sequence for BinanceSpotSeq {
	fn has_gap_from_prev(&self, prev: &Self) -> bool {
		self.u_first != prev.u_final + 1
	}
}

impl Sequence for BinancePerpSeq {
	fn has_gap_from_prev(&self, prev: &Self) -> bool {
		self.pu != prev.u_final
	}
}

impl Sequence for BinanceDepthSeq {
	fn has_gap_from_prev(&self, prev: &Self) -> bool {
		match (self, prev) {
			(Self::Spot(a), Self::Spot(b)) => a.has_gap_from_prev(b),
			(Self::Perp(a), Self::Perp(b)) => a.has_gap_from_prev(b),
			_ => panic!("BinanceDepthSeq variant mismatch: spot/perp on the same connection"),
		}
	}
}
//,}}}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn spot_seq_gap() {
		let prev = BinanceSpotSeq { u_first: 10, u_final: 20 };
		let next_ok = BinanceSpotSeq { u_first: 21, u_final: 30 };
		let next_gap = BinanceSpotSeq { u_first: 22, u_final: 30 };
		assert!(!next_ok.has_gap_from_prev(&prev));
		assert!(next_gap.has_gap_from_prev(&prev));
	}

	#[test]
	fn perp_seq_gap() {
		let prev = BinancePerpSeq { u_first: 10, u_final: 20, pu: 5 };
		let next_ok = BinancePerpSeq { u_first: 21, u_final: 30, pu: 20 };
		let next_gap = BinancePerpSeq { u_first: 21, u_final: 30, pu: 19 };
		assert!(!next_ok.has_gap_from_prev(&prev));
		assert!(next_gap.has_gap_from_prev(&prev));
	}
}
