use std::backtrace::Backtrace;

use adapters::generics::{
	http::{ApiError, AuthError, HandleError, IpError, RequestError},
	ws::WsError,
};
use v_utils::{
	prelude::*,
	trades::Timeframe,
	utils::{Sysexit, SysexitCode},
};

use crate::{ExchangeName, Instrument};

// Exchange Error {{{
pub type ExchangeResult<T> = Result<T, Error>;

impl From<RequestError> for Error {
	fn from(e: RequestError) -> Self {
		match e {
			RequestError::HandleResponse(HandleError::Api(ApiError::Auth(auth))) => Self::Auth(auth),
			RequestError::HandleResponse(HandleError::Api(ApiError::Ip(ip))) => Self::Ip(ip),
			other => Self::Request(other),
		}
	}
}

/// Re-export as ExchangeError for internal use to avoid rewrites
pub use Error as ExchangeError;

#[derive(Debug, miette::Diagnostic, derive_more::Display, thiserror::Error, derive_more::From)]
pub enum Error {
	/// exchange-specific or method-specific stuff
	#[diagnostic(transparent)]
	#[from(skip)]
	Request(RequestError),
	/// Specific to websocket
	#[diagnostic(transparent)]
	Ws(WsError),

	// commonly understood cases {{{1
	#[diagnostic(transparent)]
	Timeframe(UnsupportedTimeframeError),
	#[diagnostic(transparent)]
	Range(RequestRangeError),
	#[diagnostic(transparent)]
	Auth(AuthError),
	#[diagnostic(transparent)]
	Ip(IpError),
	//,}}}1
	/// our internal markings
	#[diagnostic(transparent)]
	Method(MethodError),
	#[diagnostic(code(v_exchanges::other))]
	Other(Report),
}

impl SysexitCode for Error {
	fn sysexit(&self) -> Sysexit {
		match self {
			Self::Auth(_) | Self::Ip(_) => Sysexit::NoPerm,
			_ => Sysexit::None,
		}
	}
}

#[derive(Debug, miette::Diagnostic, thiserror::Error, derive_new::new)]
#[error("Chosen exchange does not support the requested timeframe. Provided: {provided}, allowed: {allowed:?}")]
#[diagnostic(code(v_exchanges::unsupported_timeframe), help("Use one of the allowed timeframes for this exchange."))]
pub struct UnsupportedTimeframeError {
	provided: Timeframe,
	allowed: Vec<Timeframe>,
	#[new(value = "Backtrace::capture()")]
	backtrace: Backtrace,
}

#[derive(Debug, miette::Diagnostic, thiserror::Error, derive_new::new)]
pub enum MethodError {
	/// Means that it's **not expected** to be implemented, not only that it's not implemented now. For things that are yet to be implemented I just put `unimplemented!()`.
	#[error("Method not implemented for the requested exchange and instrument: ({exchange}, {instrument})")]
	#[diagnostic(
		code(v_exchanges::method::not_implemented),
		help("This method is not expected to be implemented for this exchange/instrument combination.")
	)]
	MethodNotImplemented {
		exchange: ExchangeName,
		instrument: Instrument,
		#[new(value = "Backtrace::capture()")]
		backtrace: Backtrace,
	},
	#[error("Requested exchange does not support the method for chosen instrument: ({exchange}, {instrument})")]
	#[diagnostic(code(v_exchanges::method::not_supported), help("This exchange does not support this method for the specified instrument type."))]
	MethodNotSupported {
		exchange: ExchangeName,
		instrument: Instrument,
		#[new(value = "Backtrace::capture()")]
		backtrace: Backtrace,
	},
}

#[derive(Debug, miette::Diagnostic, derive_more::Display, thiserror::Error, derive_more::From)]
pub enum RequestRangeError {
	#[diagnostic(transparent)]
	OutOfRange(OutOfRangeError),
	#[diagnostic(code(v_exchanges::range::other))]
	Others(Report),
}

#[derive(derive_more::Debug, miette::Diagnostic, thiserror::Error, derive_new::new)]
#[error("Effective provided limit is out of range (could be translated from Start:End / tf). Allowed: {allowed:?}, provided: {provided}")]
#[diagnostic(code(v_exchanges::range::out_of_range), help("Adjust the request parameters to fall within the allowed range."))]
pub struct OutOfRangeError {
	allowed: std::ops::RangeInclusive<u32>,
	provided: u32,
	#[new(value = "Backtrace::capture()")]
	backtrace: Backtrace,
}
//,}}}
