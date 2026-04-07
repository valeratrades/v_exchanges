use arrayvec::ArrayString;
use jiff::Timestamp;
use smart_default::SmartDefault;
use uuid::Uuid;
use v_utils::{arch::ComponentState, trades::Side};

use crate::Ticker;

/// An order bound to a specific exchange and ticker, ready to be placed.
#[derive(Clone, Debug, derive_more::Deref, derive_more::DerefMut, PartialEq, derive_new::new)]
pub struct ExchangeOrder<O> {
	#[deref]
	#[deref_mut]
	pub order: O,
	pub ticker: Ticker,
	#[new(default)]
	pub expected_fee_usd: Option<f64>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, SmartDefault)]
pub struct OrderId {
	#[default(Uuid::now_v7())]
	pub id: Uuid,
	pub parent: Option<Uuid>,
	pub exchange_id: Option<ArrayString<32>>,
}

/// Exchange-agnostic limit order.
///
/// All fields beyond the core (side, price, qty) default to sensible values.
/// Each exchange adapter is responsible for validating and translating these into exchange-specific parameters.
#[derive(Clone, Debug, PartialEq, derive_new::new)]
pub struct LimitOrder {
	pub side: Side,
	pub price: f64,
	pub qty: f64, //Q: should I make order be generic over the qty? Or maybe just Decimal?
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
	pub order_id: OrderId,
	#[new(default)]
	pub contingency: Option<Contingency>,
	#[new(default)]
	pub tags: Vec<ArrayString<32>>,
	//TODO: I think we need a consistent generic way to tag an order.
	//Q: how do I make it not only id itself, but also allow for including info of its parent strategy

	//Q: nautilus has `quote_quantity: bool`. Do I want it? Or should I on the contrary avoid it as plague, for fear of overcomplicating the logic?
}
impl Eq for LimitOrder {}
impl std::hash::Hash for LimitOrder {
	fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
		self.side.hash(state);
		self.price.to_bits().hash(state);
		self.qty.to_bits().hash(state);
		self.time_in_force.hash(state);
		self.post_only.hash(state);
		self.reduce_only.hash(state);
		self.display_qty.map(f64::to_bits).hash(state);
		self.trigger.hash(state);
		self.stp.hash(state);
		self.order_id.hash(state);
		self.contingency.hash(state);
		self.tags.hash(state);
	}
}

/// Exchange-agnostic market order.
#[derive(Clone, Debug, derive_new::new)]
pub struct MarketOrder {
	pub side: Side,
	pub qty: f64,
	#[new(default)]
	pub reduce_only: bool,
	#[new(default)]
	pub stp: Option<SelfTradePreventionMode>,
	#[new(default)]
	pub order_id: OrderId,
}

/// Stop-limit order: a limit order that activates when the trigger price is hit.
#[derive(Clone, Debug, derive_new::new)]
pub struct StopLimitOrder {
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
	pub order_id: OrderId,
}

/// Stop-market order: a market order that activates when the trigger price is hit.
#[derive(Clone, Debug, derive_new::new)]
pub struct StopMarketOrder {
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
	pub order_id: OrderId,
}

/// Trailing stop-market order.
#[derive(Clone, Debug, derive_new::new)]
pub struct TrailingStopOrder {
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
	pub order_id: OrderId,
}

/// Trigger configuration for conditional orders (stop-limit, stop-market, take-profit, etc.)
#[derive(Clone, Debug, PartialEq, derive_new::new)]
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

impl Eq for Trigger {}
impl std::hash::Hash for Trigger {
	fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
		self.price.to_bits().hash(state);
		self.price_type.hash(state);
	}
}

/// What price feed triggers the conditional order.
#[derive(Clone, Copy, Debug, Default, strum::Display, Eq, Hash, PartialEq)]
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
#[derive(Clone, Copy, Debug, Default, derive_more::Display, Eq, Hash, PartialEq)]
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

/// Contingency linkage between orders.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Contingency {
	/// One-Cancels-the-Other: when one order fills or cancels, the linked order is canceled.
	Oco(Vec<Uuid>),
	/// One-Triggers-the-Other: when the parent order fills, the linked orders are submitted.
	Oto(Vec<Uuid>),
}

/// Binance: EXPIRE_MAKER/EXPIRE_TAKER/EXPIRE_BOTH; OKX: cancel_maker/cancel_taker/cancel_both
#[derive(Clone, Copy, Debug, strum::Display, Eq, Hash, PartialEq)]
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
	pub order_id: OrderId,
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
