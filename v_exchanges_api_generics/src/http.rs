use std::{fmt::Debug, time::Duration};

pub use bytes::Bytes;
pub use reqwest::{
	Method, Request, RequestBuilder, StatusCode,
	header::{self, HeaderMap},
};
use v_utils::prelude::*;

/// The User Agent string
pub static USER_AGENT: &str = concat!("v_exchanges_api_generics/", env!("CARGO_PKG_VERSION"));

/// Client for communicating with APIs through HTTP/HTTPS.
///
/// When making a HTTP request or starting a websocket connection with this client,
/// a handler that implements [RequestHandler] is required.
#[derive(Debug, Clone, Default)]
pub struct Client {
	client: reqwest::Client,
	#[doc(hidden)]
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
		let base_url = handler.base_url();
		config.verify();
		let url = base_url + url;
		debug!(?config);

		for i in 1..=config.max_tries {
			//HACK: hate to create a new request every time, but I haven't yet figured out how to provide by reference
			let mut request_builder = self.client.request(method.clone(), url.clone()).timeout(config.timeout);
			if let Some(query) = query {
				request_builder = request_builder.query(query);
			}
			Span::current().record("request_builder", format!("{:?}", request_builder));

			let request = handler.build_request(request_builder, &body, i).map_err(RequestError::BuildRequest)?;
			match self.client.execute(request).await {
				Ok(mut response) => {
					let status = response.status();
					let headers = std::mem::take(response.headers_mut());
					let body: Bytes = response.bytes().await.map_err(RequestError::ReceiveResponse)?;
					//TODO!!!: we are only retrying when response is not received. Although there is a list of errors we would also like to retry on.
					let body_str: &str = std::str::from_utf8(&body).unwrap_or_default().trim();
					let truncated_body = v_utils::utils::truncate_msg(body_str);
					debug!(truncated_body);
					return handler.handle_response(status, headers, body).map_err(RequestError::HandleResponse);
				}
				Err(e) =>
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
	#[inline(always)]
	pub async fn get<Q, H>(&self, url: &str, query: &Q, handler: &H) -> Result<H::Successful, RequestError>
	where
		Q: Serialize + ?Sized + Debug,
		H: RequestHandler<()>, {
		self.request::<Q, (), H>(Method::GET, url, Some(query), None, handler).await
	}

	/// Derivation of [get()][Self::get()].
	#[inline(always)]
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
	#[inline(always)]
	pub async fn post<B, H>(&self, url: &str, body: B, handler: &H) -> Result<H::Successful, RequestError>
	where
		H: RequestHandler<B>, {
		self.request::<(), B, H>(Method::POST, url, None, Some(body), handler).await
	}

	/// Derivation of [post()][Self::post()].
	#[inline(always)]
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
	#[inline(always)]
	pub async fn put<B, H>(&self, url: &str, body: B, handler: &H) -> Result<H::Successful, RequestError>
	where
		H: RequestHandler<B>, {
		self.request::<(), B, H>(Method::PUT, url, None, Some(body), handler).await
	}

	/// Derivation of [put()][Self::put()].
	#[inline(always)]
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
	#[inline(always)]
	pub async fn delete<Q, H>(&self, url: &str, query: &Q, handler: &H) -> Result<H::Successful, RequestError>
	where
		Q: Serialize + ?Sized + Debug,
		H: RequestHandler<()>, {
		self.request::<Q, (), H>(Method::DELETE, url, Some(query), None, handler).await
	}

	/// Derivation of [delete()][Self::delete()].
	#[inline(always)]
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
	fn base_url(&self) -> String {
		String::default()
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
/// Should be returned by [RequestHandler::request_config()].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RequestConfig {
	/// [Client] will retry sending a request if it failed to send. `max_try` can be used limit the number of attempts.
	///
	/// Do not set this to `0` or [Client::request()] will **panic**. [Default]s to `1` (which means no retry).
	pub max_tries: u8,
	/// Duration that should elapse after retrying sending a request.
	///
	/// [Default]s to 500ms. See also: `max_try`.
	pub retry_cooldown: Duration,
	/// The timeout set when sending a request. [Default]s to 3s.
	///
	/// It is possible for the [RequestHandler] to override this in [RequestHandler::build_request()].
	/// See also: [RequestBuilder::timeout()].
	pub timeout: Duration,
}

impl RequestConfig {
	#[inline(always)]
	fn verify(&self) {
		assert_ne!(self.max_tries, 0, "RequestConfig.max_tries must not be equal to 0");
	}
}
impl Default for RequestConfig {
	fn default() -> Self {
		Self {
			max_tries: 1,
			retry_cooldown: Duration::from_millis(500),
			timeout: Duration::from_secs(3),
		}
	}
}

/// Error type encompassing all the failure modes of [RequestHandler::handle_response()].
#[derive(Error, Debug, derive_more::Display, derive_more::From)]
pub enum HandleError {
	/// Refer to [ApiError]
	Api(ApiError),
	/// Couldn't parse the response. Most often will wrap a [serde_json::Error].
	Parse(serde_json::Error),
	#[allow(missing_docs)]
	Other(Report),
}
/// Errors that exchanges purposefully transmit.
#[derive(Error, Debug, derive_more::Display, derive_more::From)]
pub enum ApiError {
	/// Ip has been timed out or banned
	IpTimeout {
		/// Time of unban
		until: DateTime<Utc>,
	},
	/// Errors that are a) specific to a particular exchange or b) should be handled by this crate, but are here for dev convenience
	Other(Report),
}

/// An `enum` that represents errors that could be returned by [Client::request()]
#[derive(Error, Debug)]
pub enum RequestError {
	/// An error which occurred while sending a HTTP request.
	#[error("failed to send HTTP request: {0}")]
	SendRequest(#[source] reqwest::Error),
	/// An error which occurred while receiving a HTTP response.
	#[error("failed to receive HTTP response: {0}")]
	ReceiveResponse(#[source] reqwest::Error),
	/// Error occurred in [RequestHandler::build_request()].
	#[error("the handler failed to build a request: {0}")]
	BuildRequest(BuildError),
	/// An error which was returned by [RequestHandler::handle_response()].
	#[error("the handler returned an error: {0}")]
	HandleResponse(HandleError),
	#[allow(missing_docs)]
	#[error("{0}")]
	Other(Report),
}

/// Errors that can occur during exchange's implementation of the build-request process.
#[derive(Error, Debug, derive_more::From, derive_more::Display)]
pub enum BuildError {
	/// signed request attempted, while lacking one of the necessary auth fields
	Auth(MissingAuth),
	/// could not serialize body as application/x-www-form-urlencoded
	UrlSerialization(serde_urlencoded::ser::Error),
	#[allow(missing_docs)]
	Other(Report),
}

#[derive(Error, Debug, derive_more::Display, derive_more::From)]
pub enum MissingAuth {
	ApiKey,
	SecretKey,
}
