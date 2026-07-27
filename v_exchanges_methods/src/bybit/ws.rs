use std::collections::BTreeMap;

use adapters::{
	Client,
	bybit::{BybitOption, BybitWsHandler, BybitWsUrlBase},
	generics::ws::{WsConnection, WsError},
};
use jiff::Timestamp;
use trading_data_core::{Accumulator, BatchTrades, InnerTrade, Pair, Side, Span, Ts};

use crate::{BookShape, BookUpdate, ExchangeStream, Instrument, PrecisionPriceQty, core::Sequence};

// book {{{
#[derive(Debug)]
pub struct BookConnection {
	connection: WsConnection<BybitWsHandler>,
	pair_precisions: BTreeMap<Pair, PrecisionPriceQty>,
	/// Last seq seen per pair on the live delta chain. Used to log a gap warning when the
	/// per-symbol `u` is non-contiguous (excluding snapshot boundaries).
	last_seq: BTreeMap<Pair, BybitSeq>,
}
impl BookConnection {
	pub fn try_new(client: &Client, pairs: &[Pair], instrument: Instrument, pair_precisions: BTreeMap<Pair, PrecisionPriceQty>) -> Result<Self, WsError> {
		let vec_topic_str = pairs.iter().map(|p| format!("orderbook.1000.{}", p.fmt_bybit())).collect::<Vec<_>>();

		let url_suffix = match instrument {
			Instrument::Perp => "/v5/public/linear",
			Instrument::Spot => "/v5/public/spot",
			_ => unimplemented!(),
		};
		let connection = client.ws_connection(url_suffix, vec![BybitOption::WsUrl(BybitWsUrlBase::Bybit), BybitOption::WsTopics(vec_topic_str)])?;

		Ok(Self {
			connection,
			pair_precisions,
			last_seq: BTreeMap::new(),
		})
	}
}
#[async_trait::async_trait]
impl ExchangeStream for BookConnection {
	type Item = BookUpdate;

	async fn next(&mut self) -> Result<Vec<Self::Item>, WsError> {
		let batch = self.connection.next().await?;
		let mut out = Vec::with_capacity(batch.len());
		for content_event in batch {
			let parsed: BybitBookData = serde_json::from_value(content_event.data).expect("Exchange responded with invalid book event");

			// topic: "orderbook.1000.BTCUSDT" → last '.'-segment → "BTCUSDT"
			let pair_str = content_event.topic.rsplit('.').next().expect("Bybit orderbook topic always contains '.'");
			let pair: Pair = pair_str
				.try_into()
				.unwrap_or_else(|_| panic!("failed to parse pair from orderbook topic: {}", content_event.topic));
			let prec = *self.pair_precisions.get(&pair).unwrap_or_else(|| panic!("{pair} not in pair_precisions"));

			let parse_level = |(p, q): (String, String)| -> (i32, u32) { (prec.parse_price(&p), prec.parse_qty(&q)) };
			let is_snapshot = match content_event.event_type.as_str() {
				"snapshot" => true,
				"delta" => false,
				other => panic!("Bybit sent unexpected book event type: {other}"),
			};
			let seq = BybitSeq { u: parsed.u, is_snapshot };
			let gapped = self.last_seq.get(&pair).map(|prev| seq.has_gap_from_prev(prev)).unwrap_or(false);
			if gapped {
				tracing::warn!(pair = %pair, next_u = seq.u, "Bybit orderbook gap detected on delta chain");
			}
			self.last_seq.insert(pair, seq);

			// `cts` is the matching-engine time and `ts` the envelope; Bybit reports both, so both
			// are kept. `cts` is absent on the (undocumented) frames that omit it, and there the
			// execution reading is genuinely unknown rather than equal to the send.
			let venue_send = Ts::from(content_event.time);
			let shape = BookShape {
				ts: Accumulator {
					venue: Span::at(content_event.exec_time.map(Ts::from).unwrap_or(venue_send)),
					// Stamped by the consumer at ingest; the adapter has no place in the local chain.
					local: None,
				},
				venue_send: Some(venue_send),
				prec,
				bids: parsed.b.into_iter().map(parse_level).collect(),
				asks: parsed.a.into_iter().map(parse_level).collect(),
			};
			out.push(if is_snapshot {
				BookUpdate::Snapshot(shape)
			} else {
				BookUpdate::BatchDelta { shape, gapped }
			});
		}
		Ok(out)
	}
}

// trades {{{
#[derive(Debug)]
pub struct TradeConnection {
	connection: WsConnection<BybitWsHandler>,
	pair_precisions: BTreeMap<Pair, PrecisionPriceQty>,
}
impl TradeConnection {
	pub fn try_new(client: &Client, pairs: &[Pair], instrument: Instrument, pair_precisions: BTreeMap<Pair, PrecisionPriceQty>) -> Result<Self, WsError> {
		let vec_topic_str = pairs.iter().map(|p| format!("publicTrade.{}", p.fmt_bybit())).collect::<Vec<_>>();

		let url_suffix = match instrument {
			Instrument::Perp => "/v5/public/linear",
			Instrument::Spot => "/v5/public/spot",
			_ => unimplemented!(),
		};
		let connection = client.ws_connection(url_suffix, vec![BybitOption::WsUrl(BybitWsUrlBase::Bybit), BybitOption::WsTopics(vec_topic_str)])?;

		Ok(Self { connection, pair_precisions })
	}
}
#[async_trait::async_trait]
impl ExchangeStream for TradeConnection {
	type Item = BatchTrades;

	async fn next(&mut self) -> Result<Vec<Self::Item>, WsError> {
		let batch = self.connection.next().await?;
		// One `now` per socket read: every frame drained here shares a reception time, and a
		// per-frame `now()` would only record scheduler jitter as if it were network latency.
		let now = Ts::from(Timestamp::now());
		let mut by_pair: BTreeMap<Pair, (PrecisionPriceQty, Vec<InnerTrade>)> = BTreeMap::new();
		for content_event in batch {
			// Bybit sends one frame per matching batch, so the envelope `ts` is that batch's send
			// time and applies to every trade in `data`.
			let sent = Ts::from(content_event.time);
			let parsed: Vec<BybitTradeData> = serde_json::from_value(content_event.data).expect("Exchange responded with invalid trade event");
			for t in parsed {
				let pair: Pair = t.symbol.as_str().try_into().unwrap_or_else(|_| panic!("failed to parse pair from trade event: {}", t.symbol));
				let prec = *self.pair_precisions.get(&pair).unwrap_or_else(|| panic!("{pair} not in pair_precisions"));
				by_pair.entry(pair).or_insert((prec, Vec::new())).1.push(InnerTrade {
					time: Ts::from(Timestamp::from_millisecond(t.time).expect("Exchange responded with invalid timestamp")),
					sent: Some(sent),
					price: prec.parse_price(&t.price),
					qty: prec.parse_qty(&t.size),
					// Bybit's `S` already names the taker, unlike Binance's maker-flag.
					side: t.side,
				});
			}
		}
		// `BatchTrades::new` asserts non-empty and venue-time sorted; a frame with no trades simply
		// contributes no group, so an all-empty read yields `Ok(vec![])`.
		Ok(by_pair
			.into_iter()
			.map(|(_, (prec, mut trades))| {
				trades.sort_by_key(|t| t.time);
				BatchTrades::new(prec, trades, now)
			})
			.collect())
	}
}

/// Sequence token for Bybit v5 orderbook events. `is_snapshot` disables the gap check across a
/// snapshot boundary (either side being a snapshot resets the chain).
#[derive(Clone, Copy, Debug)]
pub struct BybitSeq {
	pub u: u64,
	pub is_snapshot: bool,
}
/// Bybit public trade event entry.
/// Docs: https://bybit-exchange.github.io/docs/v5/websocket/public/trade
#[derive(Clone, Debug, serde::Deserialize)]
struct BybitTradeData {
	/// Trade fill time (venue matching engine).
	#[serde(rename = "T")]
	time: i64,
	#[serde(rename = "s")]
	symbol: String,
	/// Taker side.
	#[serde(rename = "S")]
	side: Side,
	#[serde(rename = "v")]
	size: String,
	#[serde(rename = "p")]
	price: String,
}
//,}}}

/// Bybit orderbook event data payload.
/// Docs: https://bybit-exchange.github.io/docs/v5/websocket/public/orderbook
#[derive(Clone, Debug, serde::Deserialize)]
struct BybitBookData {
	/// Bids: [[price, qty], ...]
	b: Vec<(String, String)>,
	/// Asks: [[price, qty], ...]
	a: Vec<(String, String)>,
	/// Per-symbol update id; resets to 1 on snapshots.
	u: u64,
}

impl Sequence for BybitSeq {
	fn has_gap_from_prev(&self, prev: &Self) -> bool {
		if self.is_snapshot || prev.is_snapshot {
			return false;
		}
		self.u != prev.u + 1
	}
}
//,}}}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn delta_chain_no_gap() {
		let prev = BybitSeq { u: 100, is_snapshot: false };
		let next = BybitSeq { u: 101, is_snapshot: false };
		assert!(!next.has_gap_from_prev(&prev));
	}

	#[test]
	fn delta_chain_gap() {
		let prev = BybitSeq { u: 100, is_snapshot: false };
		let next = BybitSeq { u: 102, is_snapshot: false };
		assert!(next.has_gap_from_prev(&prev));
	}

	#[test]
	fn snapshot_boundary_never_gapped() {
		// fresh snapshot after a delta
		let prev = BybitSeq { u: 100, is_snapshot: false };
		let next = BybitSeq { u: 1, is_snapshot: true };
		assert!(!next.has_gap_from_prev(&prev));

		// first delta after a snapshot, even with non-contiguous u
		let prev = BybitSeq { u: 1, is_snapshot: true };
		let next = BybitSeq { u: 500, is_snapshot: false };
		assert!(!next.has_gap_from_prev(&prev));
	}
}
