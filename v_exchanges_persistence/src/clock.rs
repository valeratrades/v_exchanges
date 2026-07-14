//! Injectable clock for `ts_init` stamping. Live code uses [`LiveClock`]; tests can substitute
//! a deterministic implementation. A clock that returns the same value across a batch of pushes
//! also unlocks future "batched write" optimizations.

/// Returns a UNIX nanosecond timestamp. Implementations must be cheap — called on every WS event.
pub trait Clock: Send + Sync {
	fn now_ns(&self) -> i64;
}

/// Live wall-clock backed by [`jiff::Timestamp::now`].
pub struct LiveClock;

impl Clock for LiveClock {
	fn now_ns(&self) -> i64 {
		jiff::Timestamp::now().as_nanosecond() as i64
	}
}
