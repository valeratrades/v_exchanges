use adapters::{
	Client,
	bybit::{BybitOption, BybitWsHandler, BybitWsUrlBase},
	generics::ws::{WsConnection, WsError},
};
use v_utils::trades::Pair;

use crate::{BookShape, BookUpdate, ExchangeStream, Instrument};

// book {{{
#[derive(Debug, derive_more::Deref, derive_more::DerefMut)]
pub struct BookConnection {
	#[deref]
	#[deref_mut]
	connection: WsConnection<BybitWsHandler>,
}
impl BookConnection {
	pub fn new(client: &Client, pairs: Vec<Pair>, instrument: Instrument) -> Result<Self, WsError> {
		let vec_topic_str = pairs.into_iter().map(|p| format!("orderbook.1000.{}", p.fmt_bybit())).collect::<Vec<_>>();

		let url_suffix = match instrument {
			Instrument::Perp => "/v5/public/linear",
			Instrument::Spot => "/v5/public/spot",
			_ => unimplemented!(),
		};
		let connection = client.ws_connection(url_suffix, vec![BybitOption::WsUrl(BybitWsUrlBase::Bybit), BybitOption::WsTopics(vec_topic_str)])?;

		Ok(Self { connection })
	}
}
#[async_trait::async_trait]
impl ExchangeStream for BookConnection {
	type Item = BookUpdate;

	async fn next(&mut self) -> Result<Self::Item, WsError> {
		let content_event = self.connection.next().await?;
		let parsed: BybitBookData = serde_json::from_value(content_event.data.clone()).expect("Exchange responded with invalid book event");
		let shape = BookShape {
			time: content_event.time,
			bids: parsed.b.into_iter().map(|(p, q)| (p.parse().unwrap(), q.parse().unwrap())).collect(),
			asks: parsed.a.into_iter().map(|(p, q)| (p.parse().unwrap(), q.parse().unwrap())).collect(),
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
