use adapters::generics::{http::RequestError, ws::WsError};
use v_utils::{prelude::*, trades::Timeframe};

use crate::{ExchangeName, Instrument};

// Exchange Error {{{
pub type ExchangeResult<T> = Result<T, Error>;

#[derive(Debug, miette::Diagnostic, derive_more::Display, thiserror::Error, derive_more::From)]
pub enum Error {
	#[diagnostic(transparent)]
	Request(RequestError),
	#[diagnostic(transparent)]
	Method(MethodError),
	#[diagnostic(transparent)]
	Timeframe(UnsupportedTimeframeError),
	#[diagnostic(transparent)]
	Ws(WsError),
	#[diagnostic(transparent)]
	Range(RequestRangeError),
	#[diagnostic(code(v_exchanges::other))]
	Other(Report),
}

/// Re-export as ExchangeError for internal use to avoid rewrites
pub use Error as ExchangeError;

#[derive(Debug, miette::Diagnostic, thiserror::Error, derive_new::new)]
#[error("Chosen exchange does not support the requested timeframe. Provided: {provided}, allowed: {allowed:?}")]
#[diagnostic(code(v_exchanges::unsupported_timeframe), help("Use one of the allowed timeframes for this exchange."))]
pub struct UnsupportedTimeframeError {
	provided: Timeframe,
	allowed: Vec<Timeframe>,
}

#[derive(Debug, miette::Diagnostic, thiserror::Error, derive_new::new)]
pub enum MethodError {
	/// Means that it's **not expected** to be implemented, not only that it's not implemented now. For things that are yet to be implemented I just put `unimplemented!()`.
	#[error("Method not implemented for the requested exchange and instrument: ({exchange}, {instrument})")]
	#[diagnostic(
		code(v_exchanges::method::not_implemented),
		help("This method is not expected to be implemented for this exchange/instrument combination.")
	)]
	MethodNotImplemented { exchange: ExchangeName, instrument: Instrument },
	#[error("Requested exchange does not support the method for chosen instrument: ({exchange}, {instrument})")]
	#[diagnostic(code(v_exchanges::method::not_supported), help("This exchange does not support this method for the specified instrument type."))]
	MethodNotSupported { exchange: ExchangeName, instrument: Instrument },
}

#[derive(Debug, miette::Diagnostic, derive_more::Display, thiserror::Error, derive_more::From)]
pub enum RequestRangeError {
	#[diagnostic(transparent)]
	OutOfRange(OutOfRangeError),
	#[diagnostic(code(v_exchanges::range::other))]
	Others(Report),
}

#[derive(derive_more::Debug, miette::Diagnostic, thiserror::Error, derive_new::new)]
#[diagnostic(code(v_exchanges::range::out_of_range), help("Adjust the request parameters to fall within the allowed range."))]
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
