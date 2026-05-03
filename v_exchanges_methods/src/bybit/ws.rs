use std::collections::BTreeMap;

use adapters::{
	Client,
	bybit::{BybitOption, BybitWsHandler, BybitWsUrlBase},
	generics::ws::{WsConnection, WsError},
};
use jiff::Timestamp;
use v_utils::trades::Pair;

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

	async fn next(&mut self) -> Result<Self::Item, WsError> {
		let content_event = self.connection.next().await?;
		let parsed: BybitBookData = serde_json::from_value(content_event.data.clone()).expect("Exchange responded with invalid book event");

		// topic: "orderbook.1000.BTCUSDT" → last '.'-segment → "BTCUSDT"
		let pair_str = content_event.topic.rsplit('.').next().expect("Bybit orderbook topic always contains '.'");
		let pair: Pair = pair_str
			.try_into()
			.unwrap_or_else(|_| panic!("failed to parse pair from orderbook topic: {}", content_event.topic));
		let prec = *self.pair_precisions.get(&pair).unwrap_or_else(|| panic!("{pair} not in pair_precisions"));

		let parse_level = |(p, q): (String, String)| -> (i32, u32) { (prec.parse_price(&p), prec.parse_qty(&q)) };
		let now = Timestamp::now();
		let is_snapshot = match content_event.event_type.as_str() {
			"snapshot" => true,
			"delta" => false,
			other => panic!("Bybit sent unexpected book event type: {other}"),
		};
		let seq = BybitSeq { u: parsed.u, is_snapshot };
		if let Some(prev) = self.last_seq.get(&pair)
			&& seq.has_gap_from_prev(prev)
		{
			tracing::warn!(pair = %pair, prev_u = prev.u, next_u = seq.u, "Bybit orderbook gap detected on delta chain");
		}
		self.last_seq.insert(pair, seq);

		let shape = BookShape {
			ts_event: content_event.time,
			ts_init: now,
			ts_last: now,
			prec,
			bids: parsed.b.into_iter().map(parse_level).collect(),
			asks: parsed.a.into_iter().map(parse_level).collect(),
		};
		if is_snapshot { Ok(BookUpdate::Snapshot(shape)) } else { Ok(BookUpdate::BatchDelta(shape)) }
	}
}

/// Sequence token for Bybit v5 orderbook events. `is_snapshot` disables the gap check across a
/// snapshot boundary (either side being a snapshot resets the chain).
#[derive(Clone, Copy, Debug)]
pub struct BybitSeq {
	pub u: u64,
	pub is_snapshot: bool,
}
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
