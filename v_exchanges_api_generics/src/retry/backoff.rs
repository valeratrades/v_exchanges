//! Exponential backoff mechanism with jitter support.
//!
//! The backoff mechanism allows the delay to grow exponentially up to a configurable
//! maximum, optionally applying random jitter to avoid synchronized reconnection storms.
//! An "immediate first" flag is available so that the very first reconnect attempt
//! can occur without any delay.

use std::time::Duration;

use eyre::{Result, ensure};
use rand::RngExt as _;

use super::RetryConfig;

#[derive(Clone, Debug)]
pub struct ExponentialBackoff {
	/// The initial backoff delay.
	delay_initial: Duration,
	/// The maximum delay to cap the backoff.
	delay_max: Duration,
	/// The current backoff delay.
	delay_current: Duration,
	/// The factor to multiply the delay on each iteration.
	factor: f64,
	/// The maximum random jitter to add (in milliseconds).
	jitter_ms: u64,
	/// If true, the first call to `next_duration()` returns zero delay (immediate reconnect).
	immediate_reconnect: bool,
	/// The original value of `immediate_reconnect` for reset purposes.
	immediate_reconnect_original: bool,
}

/// An exponential backoff mechanism with optional jitter and immediate-first behavior.
///
/// This struct computes successive delays for reconnect attempts.
/// It starts from an initial delay and multiplies it by a factor on each iteration,
/// capping the delay at a maximum value. Random jitter is added (up to a configured
/// maximum) to the delay. When `immediate_first` is true, the first call to `next_duration`
/// returns zero delay, triggering an immediate reconnect, after which the immediate flag is disabled.
impl ExponentialBackoff {
	/// Creates a new [`ExponentialBackoff`] instance.
	///
	/// # Errors
	///
	/// Returns an error if:
	/// - `delay_initial` is zero.
	/// - `delay_max` is less than `delay_initial`.
	/// - `delay_max` exceeds `Duration::from_nanos(u64::MAX)` (≈584 years).
	/// - `factor` is not in the range [1.0, 100.0] (to prevent reconnect spam).
	pub fn try_new(delay_initial: Duration, delay_max: Duration, factor: f64, jitter_ms: u64, immediate_first: bool) -> Result<Self> {
		ensure!(!delay_initial.is_zero(), "delay_initial must be non-zero");
		ensure!(delay_max >= delay_initial, "delay_max must be >= delay_initial");
		ensure!(delay_max.as_nanos() <= u128::from(u64::MAX), "delay_max exceeds maximum representable duration (≈584 years)");
		ensure!((1.0..=100.0).contains(&factor), "factor must be in range [1.0, 100.0], got {factor}");

		Ok(Self {
			delay_initial,
			delay_max,
			delay_current: delay_initial,
			factor,
			jitter_ms,
			immediate_reconnect: immediate_first,
			immediate_reconnect_original: immediate_first,
		})
	}

	/// Return the next backoff delay with jitter and update the internal state.
	///
	/// If the `immediate_first` flag is set and this is the first call (i.e. the current
	/// delay equals the initial delay), it returns `Duration::ZERO` to trigger an immediate
	/// reconnect and disables the immediate behavior for subsequent calls.
	pub fn next_duration(&mut self) -> Duration {
		if self.immediate_reconnect && self.delay_current == self.delay_initial {
			self.immediate_reconnect = false;
			return Duration::ZERO;
		}

		// Generate random jitter
		let jitter = rand::rng().random_range(0..=self.jitter_ms);
		let delay_with_jitter = self.delay_current + Duration::from_millis(jitter);

		// Prepare the next delay with overflow protection
		// Keep all math in u128 to avoid silent truncation
		let current_nanos = self.delay_current.as_nanos();
		let max_nanos = self.delay_max.as_nanos();

		// Use checked floating point multiplication to prevent overflow
		let next_nanos_u128 = if current_nanos > u128::from(u64::MAX) {
			// Current is already at max representable value, cap to max
			max_nanos
		} else {
			let current_u64 = current_nanos as u64;
			let next_f64 = current_u64 as f64 * self.factor;

			// Check for overflow in the float result
			if next_f64 > u64::MAX as f64 { u128::from(u64::MAX) } else { u128::from(next_f64 as u64) }
		};

		let clamped = std::cmp::min(next_nanos_u128, max_nanos);
		let final_nanos = if clamped > u128::from(u64::MAX) { u64::MAX } else { clamped as u64 };

		self.delay_current = Duration::from_nanos(final_nanos);

		delay_with_jitter
	}

	/// Reset the backoff to its initial state.
	pub const fn reset(&mut self) {
		self.delay_current = self.delay_initial;
		self.immediate_reconnect = self.immediate_reconnect_original;
	}

	/// Returns the current base delay without jitter.
	#[must_use]
	pub const fn current_delay(&self) -> Duration {
		self.delay_current
	}
}

impl TryFrom<&RetryConfig> for ExponentialBackoff {
	type Error = eyre::Report;

	fn try_from(c: &RetryConfig) -> Result<Self> {
		Self::try_new(
			Duration::from_millis(c.initial_delay_ms),
			Duration::from_millis(c.max_delay_ms),
			c.backoff_factor,
			c.jitter_ms,
			c.immediate_first,
		)
	}
}

#[cfg(test)]
mod tests {
	use std::time::Duration;

	use super::*;

	#[test]
	fn test_no_jitter_exponential_growth() {
		let initial = Duration::from_millis(100);
		let max = Duration::from_millis(1600);
		let mut backoff = ExponentialBackoff::try_new(initial, max, 2.0, 0, false).unwrap();

		let d1 = backoff.next_duration();
		assert_eq!(d1, Duration::from_millis(100));

		let d2 = backoff.next_duration();
		assert_eq!(d2, Duration::from_millis(200));

		let d3 = backoff.next_duration();
		assert_eq!(d3, Duration::from_millis(400));

		let d4 = backoff.next_duration();
		assert_eq!(d4, Duration::from_millis(800));

		let d5 = backoff.next_duration();
		assert_eq!(d5, Duration::from_millis(1600));

		let d6 = backoff.next_duration();
		assert_eq!(d6, Duration::from_millis(1600));
	}

	#[test]
	fn test_reset() {
		let initial = Duration::from_millis(100);
		let max = Duration::from_millis(1600);
		let mut backoff = ExponentialBackoff::try_new(initial, max, 2.0, 0, false).unwrap();

		let _ = backoff.next_duration();
		backoff.reset();
		let d = backoff.next_duration();
		assert_eq!(d, Duration::from_millis(100));
	}

	#[test]
	fn test_jitter_within_bounds() {
		let initial = Duration::from_millis(100);
		let max = Duration::from_millis(1000);
		let jitter = 50;
		for _ in 0..10 {
			let mut backoff = ExponentialBackoff::try_new(initial, max, 2.0, jitter, false).unwrap();
			let base = backoff.delay_current;
			let delay = backoff.next_duration();
			assert!(delay >= base, "Delay {delay:?} is less than expected minimum {base:?}");
			assert!(delay <= base + Duration::from_millis(jitter), "Delay {delay:?} exceeds expected maximum");
		}
	}

	#[test]
	fn test_immediate_first() {
		let initial = Duration::from_millis(100);
		let max = Duration::from_millis(1600);
		let mut backoff = ExponentialBackoff::try_new(initial, max, 2.0, 0, true).unwrap();

		let d1 = backoff.next_duration();
		assert_eq!(d1, Duration::ZERO, "Expected immediate reconnect on first call");

		let d2 = backoff.next_duration();
		assert_eq!(d2, initial, "Expected initial delay after immediate reconnect");

		let d3 = backoff.next_duration();
		assert_eq!(d3, initial * 2, "Expected exponential growth from initial delay");
	}

	#[test]
	fn test_reset_restores_immediate_first() {
		let initial = Duration::from_millis(100);
		let max = Duration::from_millis(1600);
		let mut backoff = ExponentialBackoff::try_new(initial, max, 2.0, 0, true).unwrap();

		let d1 = backoff.next_duration();
		assert_eq!(d1, Duration::ZERO);

		let d2 = backoff.next_duration();
		assert_eq!(d2, initial);

		backoff.reset();
		let d3 = backoff.next_duration();
		assert_eq!(d3, Duration::ZERO, "Reset should restore immediate_first behavior");
	}

	#[test]
	fn test_validation_zero_initial_delay() {
		let result = ExponentialBackoff::try_new(Duration::ZERO, Duration::from_millis(1000), 2.0, 0, false);
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("delay_initial must be non-zero"));
	}

	#[test]
	fn test_validation_max_less_than_initial() {
		let result = ExponentialBackoff::try_new(Duration::from_millis(1000), Duration::from_millis(500), 2.0, 0, false);
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("delay_max must be >= delay_initial"));
	}

	#[test]
	fn test_validation_factor_out_of_range() {
		let result = ExponentialBackoff::try_new(Duration::from_millis(100), Duration::from_millis(1000), 0.5, 0, false);
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("factor"));

		let result2 = ExponentialBackoff::try_new(Duration::from_millis(100), Duration::from_millis(1000), 150.0, 0, false);
		assert!(result2.is_err());
		assert!(result2.unwrap_err().to_string().contains("factor"));
	}
}
