use jiff::Timestamp;
use v_utils::trades::{Pair, Side};

use crate::Instrument;

/// Exchange-agnostic limit order.
///
/// All fields beyond the core (pair, instrument, side, price, qty) default to sensible values.
/// Each exchange adapter is responsible for validating and translating these into exchange-specific parameters.
#[derive(Clone, Debug, derive_new::new)]
pub struct LimitOrder {
	pub pair: Pair,
	pub instrument: Instrument,
	pub side: Side,
	pub price: f64,
	pub qty: f64,
	#[new(value = "TimeInForce::Gtc")]
	pub time_in_force: TimeInForce,
	#[new(default)]
	pub post_only: bool,
	#[new(default)]
	pub reduce_only: bool,
	/// Visible quantity for iceberg orders. When set, only this amount is shown on the book; the rest is hidden.
	#[new(default)]
	pub display_qty: Option<f64>,
	#[new(default)]
	pub trigger: Option<Trigger>,
	#[new(default)]
	pub stp: Option<SelfTradePreventionMode>,
	#[new(default)]
	pub client_order_id: Option<String>,
}

/// Exchange-agnostic market order.
#[derive(Clone, Debug, derive_new::new)]
pub struct MarketOrder {
	pub pair: Pair,
	pub instrument: Instrument,
	pub side: Side,
	pub qty: f64,
	#[new(default)]
	pub reduce_only: bool,
	#[new(default)]
	pub stp: Option<SelfTradePreventionMode>,
	#[new(default)]
	pub client_order_id: Option<String>,
}

/// Stop-limit order: a limit order that activates when the trigger price is hit.
#[derive(Clone, Debug, derive_new::new)]
pub struct StopLimitOrder {
	pub pair: Pair,
	pub instrument: Instrument,
	pub side: Side,
	pub price: f64,
	pub qty: f64,
	pub trigger: Trigger,
	#[new(value = "TimeInForce::Gtc")]
	pub time_in_force: TimeInForce,
	#[new(default)]
	pub reduce_only: bool,
	#[new(default)]
	pub close_position: bool,
	#[new(default)]
	pub stp: Option<SelfTradePreventionMode>,
	#[new(default)]
	pub client_order_id: Option<String>,
}

/// Stop-market order: a market order that activates when the trigger price is hit.
#[derive(Clone, Debug, derive_new::new)]
pub struct StopMarketOrder {
	pub pair: Pair,
	pub instrument: Instrument,
	pub side: Side,
	pub qty: f64,
	pub trigger: Trigger,
	#[new(default)]
	pub reduce_only: bool,
	#[new(default)]
	pub close_position: bool,
	#[new(default)]
	pub stp: Option<SelfTradePreventionMode>,
	#[new(default)]
	pub client_order_id: Option<String>,
}

/// Trailing stop-market order.
#[derive(Clone, Debug, derive_new::new)]
pub struct TrailingStopOrder {
	pub pair: Pair,
	pub instrument: Instrument,
	pub side: Side,
	pub qty: f64,
	pub callback: TrailingCallback,
	/// Price at which the trailing mechanism activates. If None, activates immediately.
	#[new(default)]
	pub activation_price: Option<f64>,
	#[new(default)]
	pub trigger_price_type: TriggerPriceType,
	#[new(default)]
	pub reduce_only: bool,
	#[new(default)]
	pub stp: Option<SelfTradePreventionMode>,
	#[new(default)]
	pub client_order_id: Option<String>,
}

/// Trigger configuration for conditional orders (stop-limit, stop-market, take-profit, etc.)
#[derive(Clone, Debug, derive_new::new)]
pub struct Trigger {
	pub price: f64,
	#[new(default)]
	pub price_type: TriggerPriceType,
}
impl Trigger {
	pub fn last(price: f64) -> Self {
		Self {
			price,
			price_type: TriggerPriceType::Last,
		}
	}

	pub fn mark(price: f64) -> Self {
		Self {
			price,
			price_type: TriggerPriceType::Mark,
		}
	}

	pub fn index(price: f64) -> Self {
		Self {
			price,
			price_type: TriggerPriceType::Index,
		}
	}
}

/// What price feed triggers the conditional order.
#[derive(Clone, Copy, Debug, Default, strum::Display, Eq, PartialEq)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TriggerPriceType {
	/// Last traded price (Binance: CONTRACT_PRICE)
	#[default]
	Last,
	Mark,
	Index,
}

/// Trailing stop callback specification.
#[derive(Clone, Copy, Debug)]
pub enum TrailingCallback {
	/// Percentage-based callback rate (e.g. 1.0 = 1%)
	Percent(f64),
	/// Absolute price offset
	Price(f64),
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, derive_more::Display, Eq, PartialEq)]
pub enum TimeInForce {
	/// Good-Til-Canceled: remains active until filled or canceled.
	#[default]
	#[display("GTC")]
	Gtc,
	/// Immediate-Or-Cancel: fills as much as possible immediately, cancels the rest.
	#[display("IOC")]
	Ioc,
	/// Fill-Or-Kill: must be filled entirely immediately, or canceled entirely.
	#[display("FOK")]
	Fok,
	/// All-Or-None: must be filled entirely, but unlike FOK can wait on the book.
	#[display("AON")]
	Aon,
	/// Good-Til-Date: remains active until a specified expiry time.
	#[display("GTD")]
	Gtd(Timestamp),
}

/// Binance: EXPIRE_MAKER/EXPIRE_TAKER/EXPIRE_BOTH; OKX: cancel_maker/cancel_taker/cancel_both
#[derive(Clone, Copy, Debug, strum::Display, Eq, PartialEq)]
pub enum SelfTradePreventionMode {
	#[strum(serialize = "EXPIRE_MAKER")]
	CancelMaker,
	#[strum(serialize = "EXPIRE_TAKER")]
	CancelTaker,
	#[strum(serialize = "EXPIRE_BOTH")]
	CancelBoth,
}

/// Unified response from placing any order.
#[derive(Clone, Debug, derive_new::new)]
pub struct OrderPlaced {
	pub exchange_order_id: String,
	pub client_order_id: Option<String>,
	pub status: OrderStatus,
}

#[derive(Clone, Copy, Debug, strum::Display, Eq, PartialEq)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderStatus {
	New,
	PartiallyFilled,
	Filled,
	Canceled,
	Expired,
	Rejected,
}
