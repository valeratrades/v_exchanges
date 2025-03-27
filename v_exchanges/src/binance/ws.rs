use chrono::{DateTime, Utc};
use v_utils::trades::Side;
#[derive(Clone, Debug, Default, derive_new::new, Copy)]
/// Always is from the perspective of the Taker
pub struct TradeEvent {
	pub time: DateTime<Utc>,
	pub side: Side,
	pub qty_asset: f64,
	pub price: f64,
}
