use std::{fmt::Debug, path::PathBuf, sync::OnceLock, time::Duration};

pub use bytes::Bytes;
use eyre::{Report, eyre};
use jiff::Timestamp;
use reqwest::Url;
pub use reqwest::{
	Method, Request, RequestBuilder, StatusCode,
	header::{self, HeaderMap},
};
use serde::Serialize;
use tracing::{Span, debug, error, field::Empty, info, instrument, warn};

use crate::{AuthError, UrlError};

/// The User Agent string
pub static USER_AGENT: &str = concat!("v_exchanges_api_generics/", env!("CARGO_PKG_VERSION"));

/// Client for communicating with APIs through HTTP/HTTPS.
///
/// When making a HTTP request or starting a websocket connection with this client,
/// a handler that implements [RequestHandler] is required.
#[derive(Clone, Debug, Default)]
pub struct Client {
	client: reqwest::Client,
	pub config: RequestConfig,
}

impl Client {
	/// Makes an HTTP request with the given [RequestHandler] and returns the response.
	///
	/// It is recommended to use methods like [get()][Self::get()] because this method takes many type parameters and parameters.
	///
	/// The request is passed to `handler` before being sent, and the response is passed to `handler` before being returned.
	/// Note, that as stated in the docs for [RequestBuilder::query()], parameter `query` only accepts a **sequence of** key-value pairs.
	#[instrument(skip_all, fields(?url, ?query, request_builder = Empty))] //TODO: get all generics to impl std::fmt::Debug
	pub async fn request<Q, B, H>(&self, method: Method, url: &str, query: Option<&Q>, body: Option<B>, handler: &H) -> Result<H::Successful, RequestError>
	where
		Q: Serialize + ?Sized + std::fmt::Debug,
		H: RequestHandler<B>, {
		let config = &self.config;
		config.verify();
		let base_url = handler.base_url(config.use_testnet)?;
		let url = base_url.join(url).map_err(|_| RequestError::Other(eyre!("Failed to parse provided URL")))?;
		debug!(?config);

		for i in 1..=config.max_tries {
			//HACK: hate to create a new request every time, but I haven't yet figured out how to provide by reference
			let mut request_builder = self.client.request(method.clone(), url.clone()).timeout(config.timeout);
			if let Some(query) = query {
				request_builder = request_builder.query(query);
			}
			Span::current().record("request_builder", format!("{request_builder:?}"));

			if config.use_testnet
				&& let Some(cache_duration) = config.cache_testnet_calls
			{
				let path = test_calls_path(&url, &query);
				if let Ok(file) = std::fs::read_to_string(&path)
					&& path
						.metadata()
						.expect("already read the file, guaranteed to exist")
						.modified()
						.expect("switch OSes, you're on something stupid")
						.elapsed()
						.unwrap() < cache_duration
				{
					let body = Bytes::from(file);
					let (status, headers) = (StatusCode::OK, header::HeaderMap::new()); // we only cache if we get a 200 (headers are only relevant on unsuccessful), so pass defaults.
					return handler.handle_response(status, headers, body).map_err(RequestError::HandleResponse);
				}
			}

			//let (status, headers, truncated_body): (StatusCode, HeaderMap, String) = {
			let request = handler.build_request(request_builder, &body, i).map_err(RequestError::BuildRequest)?;
			match self.client.execute(request).await {
				Ok(mut response) => {
					let status = response.status();
					let headers = std::mem::take(response.headers_mut());
					debug!(?status, ?headers, "Received response headers");
					let body: Bytes = match response.bytes().await {
						Ok(b) => b,
						Err(e) => {
							error!(?status, ?headers, ?e, "Failed to read response body");
							return Err(RequestError::ReceiveResponse(e));
						}
					};
					{
						let truncated_body = v_utils::utils::truncate_msg(std::str::from_utf8(&body)?.trim());
						debug!(truncated_body);
					}

					match config.use_testnet {
						true => {
							// if we're here, the cache file didn't exist or is outdated
							let handled = handler.handle_response(status, headers.clone(), body.clone())?;
							std::fs::write(test_calls_path(&url, &query), &body).ok();
							return Ok(handled);
						}
						false => {
							return handler.handle_response(status, headers.clone(), body.clone()).map_err(|e| {
								error!(?status, ?headers, body = ?v_utils::utils::truncate_msg(std::str::from_utf8(&body).unwrap_or("<invalid utf8>")), "Failed to handle response");
								RequestError::HandleResponse(e)
							});
						}
					}
				}
				Err(e) =>
				//TODO!!!: we are only retrying when response is not received. Although there is a list of errors we would also like to retry on.
					if i < config.max_tries && e.is_timeout() {
						info!("Retrying sending request; made so far: {i}");
						tokio::time::sleep(config.retry_cooldown).await;
					} else {
						warn!(?e);
						debug!("{:?}\nAnd then trying the .is_timeout(): {}", e.status(), e.is_timeout());
						return Err(RequestError::SendRequest(e));
					},
			}
		}

		unreachable!()
	}

	/// Makes an GET request with the given [RequestHandler].
	///
	/// This method just calls [request()][Self::request()]. It requires less typing for type parameters and parameters.
	/// This method requires that `handler` can handle a request with a body of type `()`. The actual body passed will be `None`.
	///
	/// For more information, see [request()][Self::request()].
	pub async fn get<Q, H>(&self, url: &str, query: &Q, handler: &H) -> Result<H::Successful, RequestError>
	where
		Q: Serialize + ?Sized + Debug,
		H: RequestHandler<()>, {
		self.request::<Q, (), H>(Method::GET, url, Some(query), None, handler).await
	}

	/// Derivation of [get()][Self::get()].
	pub async fn get_no_query<H>(&self, url: &str, handler: &H) -> Result<H::Successful, RequestError>
	where
		H: RequestHandler<()>, {
		self.request::<&[(&str, &str)], (), H>(Method::GET, url, None, None, handler).await
	}

	/// Makes an POST request with the given [RequestHandler].
	///
	/// This method just calls [request()][Self::request()]. It requires less typing for type parameters and parameters.
	///
	/// For more information, see [request()][Self::request()].
	pub async fn post<B, H>(&self, url: &str, body: B, handler: &H) -> Result<H::Successful, RequestError>
	where
		H: RequestHandler<B>, {
		self.request::<(), B, H>(Method::POST, url, None, Some(body), handler).await
	}

	/// Derivation of [post()][Self::post()].
	pub async fn post_no_body<H>(&self, url: &str, handler: &H) -> Result<H::Successful, RequestError>
	where
		H: RequestHandler<()>, {
		self.request::<(), (), H>(Method::POST, url, None, None, handler).await
	}

	/// Makes an PUT request with the given [RequestHandler].
	///
	/// This method just calls [request()][Self::request()]. It requires less typing for type parameters and parameters.
	///
	/// For more information, see [request()][Self::request()].
	pub async fn put<B, H>(&self, url: &str, body: B, handler: &H) -> Result<H::Successful, RequestError>
	where
		H: RequestHandler<B>, {
		self.request::<(), B, H>(Method::PUT, url, None, Some(body), handler).await
	}

	/// Derivation of [put()][Self::put()].
	pub async fn put_no_body<H>(&self, url: &str, handler: &H) -> Result<H::Successful, RequestError>
	where
		H: RequestHandler<()>, {
		self.request::<(), (), H>(Method::PUT, url, None, None, handler).await
	}

	/// Makes an DELETE request with the given [RequestHandler].
	///
	/// This method just calls [request()][Self::request()]. It requires less typing for type parameters and parameters.
	/// This method requires that `handler` can handle a request with a body of type `()`. The actual body passed will be `None`.
	///
	/// For more information, see [request()][Self::request()].
	pub async fn delete<Q, H>(&self, url: &str, query: &Q, handler: &H) -> Result<H::Successful, RequestError>
	where
		Q: Serialize + ?Sized + Debug,
		H: RequestHandler<()>, {
		self.request::<Q, (), H>(Method::DELETE, url, Some(query), None, handler).await
	}

	/// Derivation of [delete()][Self::delete()].
	pub async fn delete_no_query<H>(&self, url: &str, handler: &H) -> Result<H::Successful, RequestError>
	where
		H: RequestHandler<()>, {
		self.request::<&[(&str, &str)], (), H>(Method::DELETE, url, None, None, handler).await
	}
}

/// A `trait` which is used to process requests and responses for the [Client].
pub trait RequestHandler<B> {
	/// The type which is returned to the caller of [Client::request()] when the response was successful.
	type Successful;

	/// Produce a url prefix (if any).
	#[allow(unused_variables)]
	fn base_url(&self, is_test: bool) -> Result<url::Url, UrlError> {
		Url::parse("").map_err(UrlError::Parse)
	}

	/// Build a HTTP request to be sent.
	///
	/// Implementors have to decide how to include the `request_body` into the `builder`. Implementors can
	/// also perform other operations (such as authorization) on the request.
	fn build_request(&self, builder: RequestBuilder, request_body: &Option<B>, attempt_count: u8) -> Result<Request, BuildError>;

	/// Handle a HTTP response before it is returned to the caller of [Client::request()].
	///
	/// You can verify, parse, etc... the response here before it is returned to the caller.
	///
	/// # Examples
	/// ```
	/// # use bytes::Bytes;
	/// # use reqwest::{StatusCode, header::HeaderMap};
	/// # trait Ignore {
	/// fn handle_response(&self, status: StatusCode, _: HeaderMap, response_body: Bytes) -> Result<String, ()> {
	///     if status.is_success() {
	///         let body = std::str::from_utf8(&response_body).expect("body should be valid UTF-8").to_owned();
	///         Ok(body)
	///     } else {
	///         Err(())
	///     }
	/// }
	/// # }
	/// ```
	fn handle_response(&self, status: StatusCode, headers: HeaderMap, response_body: Bytes) -> Result<Self::Successful, HandleError>;
}

/// Configuration when sending a request using [Client].
///
/// Modified in-place later if necessary.
#[derive(Clone, Debug, Default)]
pub struct RequestConfig {
	/// [Client] will retry sending a request if it failed to send. `max_try` can be used limit the number of attempts.
	///
	/// Do not set this to `0` or [Client::request()] will **panic**. [Default]s to `1` (which means no retry).
	//TODO: change to `num_retries`, so there is no special case.
	pub max_tries: u8 = 1,
	/// Duration that should elapse after retrying sending a request.
	pub retry_cooldown: Duration = Duration::from_millis(500),
	/// The timeout set when sending a request. [Default]s to 3s.
	///
	/// It is possible for the [RequestHandler] to override this in [RequestHandler::build_request()].
	/// See also: [RequestBuilder::timeout()].
	pub timeout: Duration = Duration::from_secs(3),

	/// Make all requests in test mode
	pub use_testnet: bool,
	/// if `test` is true, then we will try to read the file with the cached result of any request to the same URL, aged less than specified [Duration]
	pub cache_testnet_calls: Option<Duration> = Some(Duration::from_days(30)),
}

impl RequestConfig {
	fn verify(&self) {
		assert_ne!(self.max_tries, 0, "RequestConfig.max_tries must not be equal to 0");
	}
}

/// Error type encompassing all the failure modes of [RequestHandler::handle_response()].
#[derive(Debug, derive_more::Display, thiserror::Error, derive_more::From)]
pub enum HandleError {
	/// Refer to [ApiError]
	Api(ApiError),
	/// Couldn't parse the response. Normally just wraps the [JsonError](serde_json::Error) with [truncate_msg](v_utils::utils::truncate_msg) around the response msg.
	Parse(Report),
}
/// Errors that exchanges purposefully transmit.
#[derive(Debug, thiserror::Error, derive_more::From)]
pub enum ApiError {
	/// Ip has been timed out or banned
	#[error("IP has been timed out or banned until {until:?}")]
	IpTimeout {
		/// Time of unban
		until: Option<Timestamp>,
	},
	/// Errors that are a) specific to a particular exchange or b) should be handled by this crate, but are here for dev convenience
	#[error("{0}")]
	Other(Report),
}

/// An `enum` that represents errors that could be returned by [Client::request()]
#[derive(Debug, thiserror::Error)]
pub enum RequestError {
	#[error("failed to send HTTP request: {0}")]
	SendRequest(#[source] reqwest::Error),
	#[error("failed to parse response body as UTF-8: {0}")]
	Utf8Error(#[from] std::str::Utf8Error),
	#[error("failed to receive HTTP response: {0}")]
	ReceiveResponse(#[source] reqwest::Error),
	#[error("handler failed to build a request: {0}")]
	BuildRequest(#[from] BuildError),
	#[error("handler failed to process the response: {0}")]
	HandleResponse(#[from] HandleError),
	#[error("{0}")]
	Url(#[from] UrlError),
	/// errors meant to be propagated to the user or the developer, thus having no defined type.
	#[allow(missing_docs)]
	#[error("{0}")]
	Other(#[from] Report),
}

/// Errors that can occur during exchange's implementation of the build-request process.
#[derive(Debug, derive_more::Display, thiserror::Error, derive_more::From)]
pub enum BuildError {
	/// Signed request attempted, while lacking one of the necessary auth fields
	Auth(AuthError),
	/// Could not serialize body as application/x-www-form-urlencoded
	UrlSerialization(serde_urlencoded::ser::Error),
	/// Could not serialize body as application/json
	JsonSerialization(serde_json::Error),
	//Q: not sure if there is ever a case when client could reach that, thus currently simply unwraping.
	///// Error when calling reqwest::RequestBuilder::build()
	//Reqwest(reqwest::Error),
	#[allow(missing_docs)]
	Other(Report),
}

static TEST_CALLS_PATH: OnceLock<PathBuf> = OnceLock::new();
fn test_calls_path<Q: Serialize>(url: &Url, query: &Option<Q>) -> PathBuf {
	let base = TEST_CALLS_PATH.get_or_init(|| v_utils::xdg_cache_dir!("test_calls"));

	let mut filename = url.to_string();
	if query.is_some() {
		filename.push('?');
		filename.push_str(&serde_urlencoded::to_string(query).unwrap_or_default());
	}
	base.join(filename)
}
