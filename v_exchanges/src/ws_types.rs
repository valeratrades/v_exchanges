use adapters::generics::ws::WsConnection;
use chrono::{DateTime, Utc};

#[derive(Clone, Debug, Default)]
pub struct TradeEvent {
	pub time: DateTime<Utc>,
	pub qty_asset: f64,
	pub price: f64,
}
