use adapters::generics::{http::RequestError, ws::WsError};
use v_utils::{prelude::*, trades::Timeframe};

use crate::{ExchangeName, Instrument};

// Exchange Error {{{
pub type ExchangeResult<T> = Result<T, Error>;

#[derive(Debug, derive_more::Display, thiserror::Error, derive_more::From)]
pub enum Error {
	Request(RequestError),
	Method(MethodError),
	Timeframe(UnsupportedTimeframeError),
	Ws(WsError),
	Range(RequestRangeError),
	Other(Report),
}

/// Re-export as ExchangeError for internal use to avoid rewrites
pub use Error as ExchangeError;

#[derive(Debug, thiserror::Error, derive_new::new)]
#[error("Chosen exchange does not support the requested timeframe. Provided: {provided}, allowed: {allowed:?}")]
pub struct UnsupportedTimeframeError {
	provided: Timeframe,
	allowed: Vec<Timeframe>,
}

#[derive(Debug, thiserror::Error, derive_new::new)]
pub enum MethodError {
	/// Means that it's **not expected** to be implemented, not only that it's not implemented now. For things that are yet to be implemented I just put `unimplemented!()`.
	#[error("Method not implemented for the requested exchange and instrument: ({exchange}, {instrument})")]
	MethodNotImplemented { exchange: ExchangeName, instrument: Instrument },
	#[error("Requested exchange does not support the method for chosen instrument: ({exchange}, {instrument})")]
	MethodNotSupported { exchange: ExchangeName, instrument: Instrument },
}

#[derive(Debug, derive_more::Display, thiserror::Error, derive_more::From)]
pub enum RequestRangeError {
	OutOfRange(OutOfRangeError),
	Others(Report),
}

#[derive(derive_more::Debug, thiserror::Error, derive_new::new)]
pub struct OutOfRangeError {
	allowed: std::ops::RangeInclusive<u32>,
	provided: u32,
}

impl std::fmt::Display for OutOfRangeError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"Effective provided limit is out of range (could be translated from Start:End / tf). Allowed: {:?}, provided: {}",
			self.allowed, self.provided
		)
	}
}
//,}}}
