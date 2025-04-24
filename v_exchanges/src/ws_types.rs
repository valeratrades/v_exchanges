use adapters::generics::ws::{Topic, WsError};
use chrono::{DateTime, Utc};

#[async_trait::async_trait]
pub trait ExchangeStream {
	type Content;
	type Topic;

	async fn next(&mut self) -> Option<Result<Self::Content, WsError>>;
	async fn subscribe(&mut self, topics: Vec<Topic>) -> Result<(), WsError>;
}

#[derive(Clone, Debug, Default)]
pub struct TradeEvent {
	pub time: DateTime<Utc>,
	pub qty_asset: f64,
	pub price: f64,
}

//dbg: placeholder, ignore contents
pub struct BookSnapshot {
	pub time: DateTime<Utc>,
	pub asks: Vec<(f64, f64)>,
	pub bids: Vec<(f64, f64)>,
}
//dbg: placeholder, ignore contents
pub struct BookDelta {
	pub time: DateTime<Utc>,
	pub asks: Vec<(f64, f64)>,
	pub bids: Vec<(f64, f64)>,
}
