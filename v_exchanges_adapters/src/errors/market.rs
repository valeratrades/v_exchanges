use std::fmt;

use derive_more::Debug;

#[derive(Debug, derive_new::new)]
pub struct LimitOutOfRangeError {
	allowed: std::ops::RangeInclusive<u32>,
	provided: u32,
}
impl fmt::Display for LimitOutOfRangeError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "Limit out of range. Allowed: {:?}, provided: {}", self.allowed, self.provided)
	}
}
impl std::error::Error for LimitOutOfRangeError {}
