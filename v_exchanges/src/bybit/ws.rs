use std::collections::BTreeMap;

use adapters::{
	Client,
	bybit::{BybitOption, BybitWsHandler, BybitWsUrlBase},
	generics::ws::{WsConnection, WsError},
};
use v_utils::trades::Pair;

use crate::{BookShape, BookUpdate, ExchangeStream, Instrument, Price, Qty};

// book {{{
#[derive(Debug)]
pub struct BookConnection {
	connection: WsConnection<BybitWsHandler>,
	pair_precisions: BTreeMap<Pair, (u8, u8)>,
}
impl BookConnection {
	pub fn try_new(client: &Client, pairs: Vec<Pair>, instrument: Instrument, pair_precisions: BTreeMap<Pair, (u8, u8)>) -> Result<Self, WsError> {
		let vec_topic_str = pairs.into_iter().map(|p| format!("orderbook.1000.{}", p.fmt_bybit())).collect::<Vec<_>>();

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
impl ExchangeStream for BookConnection {
	type Item = BookUpdate;

	async fn next(&mut self) -> Result<Self::Item, WsError> {
		let content_event = self.connection.next().await?;
		let parsed: BybitBookData = serde_json::from_value(content_event.data.clone()).expect("Exchange responded with invalid book event");

		// topic: "orderbook.1000.BTCUSDT" → last '.'-segment → "BTCUSDT"
		let pair_str = content_event.topic.rsplit('.').next().expect("Bybit orderbook topic always contains '.'");
		let pair: Pair = pair_str.try_into().unwrap_or_else(|_| panic!("failed to parse pair from orderbook topic: {}", content_event.topic));
		let &(price_prec, qty_prec) = self.pair_precisions.get(&pair).unwrap_or_else(|| panic!("{pair} not in pair_precisions"));

		let shape = BookShape {
			time: content_event.time,
			bids: parsed.b.into_iter().map(|(p, q)| (Price::from_f64(p.parse().expect("valid price string"), price_prec), Qty::from_f64(q.parse().expect("valid qty string"), qty_prec))).collect(),
			asks: parsed.a.into_iter().map(|(p, q)| (Price::from_f64(p.parse().expect("valid price string"), price_prec), Qty::from_f64(q.parse().expect("valid qty string"), qty_prec))).collect(),
		};
		match content_event.event_type.as_str() {
			"snapshot" => Ok(BookUpdate::Snapshot(shape)),
			"delta" => Ok(BookUpdate::Delta(shape)),
			other => panic!("Bybit sent unexpected book event type: {other}"),
		}
	}
}

/// Bybit orderbook event data payload.
/// Docs: https://bybit-exchange.github.io/docs/v5/websocket/public/orderbook
#[derive(Clone, Debug, serde::Deserialize)]
struct BybitBookData {
	/// Bids: [[price, qty], ...]
	b: Vec<(String, String)>,
	/// Asks: [[price, qty], ...]
	a: Vec<(String, String)>,
}
//,}}}
