// A module for communicating with the MEXC API (https://mexcdevelop.github.io/apidocs/spot/en/)

use std::{
	marker::PhantomData,
	str::FromStr,
	time::{SystemTime},
};

use chrono::Utc;
use generics::{AuthError, reqwest::Url};
use hmac::{Hmac, Mac};
use secrecy::{ExposeSecret as _, SecretString};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::Sha256;
use v_exchanges_api_generics::{http::*, ws::*};
use v_utils::prelude::*;

use crate::traits::*;

static MAX_RECV_WINDOW: u16 = 60000; // as of (2025/01/18)

/// Options that can be set when creating handlers
#[derive(Debug, Default)]
pub enum MexcOption {
	/// Does nothing
	#[default]
	Default,
	/// API key
	Pubkey(String),
	/// Api secret
	Secret(SecretString),
	/// Base url for HTTP requests
	HttpUrl(MexcHttpUrl),
	/// Authentication type for HTTP requests
	HttpAuth(MexcAuth),
	/// receive window parameter used for requests
	RecvWindow(u16),
	/// Base url for Ws connections
	WsUrl(MexcWsUrl),
	/// WsConfig used for creating WsConnections
	WsConfig(WsConfig),
}

/// A struct that represents a set of MexcOptions
#[derive(Clone, derive_more::Debug, Default)]
pub struct MexcOptions {
	/// see [MexcOption::Key]
	pub pubkey: Option<String>,
	/// see [MexcOption::Secret]
	#[debug("[REDACTED]")]
	pub secret: Option<SecretString>,
	/// see [MexcOption::HttpUrl]
	pub http_url: MexcHttpUrl,
	/// see [MexcOption::HttpAuth]
	pub http_auth: MexcAuth,
	/// see [MexcOption::RecvWindow]
	pub recv_window: Option<u16>,
	/// see [MexcOption::WsUrl]
	pub ws_url: MexcWsUrl,
	/// see [MexcOption::WsConfig]
	pub ws_config: WsConfig,
}

/// Enum that represents the base url of the MEXC REST API
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum MexcHttpUrl {
	Spot,
	SpotTest,
	Futures,
	#[default]
	None,
}
impl MexcHttpUrl {
	/// The URL that this variant represents
	#[inline(always)]
	fn as_str(&self) -> &'static str {
		match self {
			Self::Spot => "https://api.mexc.com",
			Self::SpotTest => "https://api-testnet.mexc.com",
			Self::Futures => "https://contract.mexc.com",
			Self::None => "",
		}
	}
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MexcAuth {
	Sign,
	Key,
	#[default]
	None,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MexcError {
	pub code: i32,
	pub msg: String,
}
impl From<MexcError> for ApiError {
	fn from(e: MexcError) -> Self {
		ApiError::Other(eyre!("MEXC API error: {}: {}", e.code, e.msg))
	}
}

/// A struct that implements RequestHandler
pub struct MexcRequestHandler<'a, R: DeserializeOwned> {
	options: MexcOptions,
	_phantom: PhantomData<&'a R>,
}

impl<B, R> RequestHandler<B> for MexcRequestHandler<'_, R>
where
	B: Serialize,
	R: DeserializeOwned,
{
	type Successful = R;

	fn base_url(&self, is_test: bool) -> String {
		match is_test {
			true => unimplemented!(),
			false => self.options.http_url.as_str().to_owned(),
		}
	}

	#[tracing::instrument(skip_all, fields(?builder))]
	fn build_request(&self, mut builder: RequestBuilder, request_body: &Option<B>, _: u8) -> Result<Request, BuildError> {
		if let Some(body) = request_body {
			let encoded = serde_urlencoded::to_string(body)?;
			builder = builder.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(encoded);
			//builder = builder.header(header::CONTENT_TYPE, "application/json");
		}

		if self.options.http_auth != MexcAuth::None {
			let pubkey = self.options.pubkey.as_deref().ok_or(AuthError::MissingPubkey)?;
			builder = builder.header("ApiKey", pubkey);

			let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
			let timestamp = time.as_millis();
			builder = builder.header("Request-Time", timestamp.to_string());

			if let Some(recv_window) = self.options.recv_window {
				builder = builder.header("Recv-Window", recv_window.to_string());
			}

			if self.options.http_auth == MexcAuth::Sign {
				let secret = self.options.secret.as_ref().map(|s| s.expose_secret()).ok_or(AuthError::MissingSecret)?;
				let mut hmac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();

				let mut request = builder.build().expect("My understanding is that this doesn't fail on client, so fail fast for dev");
				let param_string = if request.method() == Method::GET || request.method() == Method::DELETE {
					if let Some(body) = request_body { serde_urlencoded::to_string(body)? } else { String::new() }
				} else {
					// For POST, use body as JSON string
					String::from_utf8(request.body().and_then(|body| body.as_bytes()).unwrap_or_default().to_vec()).unwrap_or_default()
				};

				let signature_base = format!("{pubkey}{timestamp}{param_string}");
				hmac.update(signature_base.as_bytes());
				let signature = hex::encode(hmac.finalize().into_bytes());
				request.headers_mut().insert("Signature", signature.parse().unwrap());

				return Ok(request);
			}
		}
		Ok(builder.build().expect("Don't expect this to be reached by client. Same reasoning - fail fast for dev"))
	}

	fn handle_response(&self, status: StatusCode, headers: HeaderMap, response_body: Bytes) -> Result<Self::Successful, HandleError> {
		if status.is_success() {
			serde_json::from_slice(&response_body).map_err(|e| {
				tracing::debug!("Failed to parse response due to an error: {e}",);
				HandleError::Parse(e)
			})
		} else {
			//Q: does MEXC even have this, or am I just blindly copying from Binance?
			if status == 429 {
				let retry_after_sec = if let Some(value) = headers.get("Retry-After") {
					if let Ok(string) = value.to_str() {
						if let Ok(retry_after) = u32::from_str(string) {
							Some(retry_after)
						} else {
							tracing::debug!("Invalid number in Retry-After header");
							None
						}
					} else {
						tracing::debug!("Non-ASCII character in Retry-After header");
						None
					}
				} else {
					None
				};
				let e = match retry_after_sec {
					Some(s) => {
						let until = Some(Utc::now() + chrono::Duration::seconds(s as i64));
						ApiError::IpTimeout { until }.into()
					}
					None => eyre!("Could't interpret Retry-After header").into(),
				};
				return Err(e);
			}

			let api_error: MexcError = match serde_json::from_slice(&response_body) {
				Ok(parsed) => parsed,
				Err(e) => return Err(HandleError::Parse(e)),
			};
			Err(ApiError::from(api_error).into())
		}
	}
}

// Ws stuff {{{
/// A struct that implements [WsHandler]
#[derive(Debug, derive_new::new)]
pub struct MexcWsHandler {
	options: MexcOptions,
}
impl WsHandler for MexcWsHandler {
	fn config(&self) -> WsConfig {
		let mut config = self.options.ws_config.clone();
		if self.options.ws_url != MexcWsUrl::None {
			config.base_url = Some(Url::parse(self.options.ws_url.as_str()).expect("urls are hardcoded and should be valid"));
		}
		config
	}
}
/// Enum that represents the base url of the MEXC Ws API
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum MexcWsUrl {
	Spot,
	SpotTest,
	Futures,
	#[default]
	None,
}
impl MexcWsUrl {
	/// The URL that this variant represents
	#[inline(always)]
	pub fn as_str(&self) -> &'static str {
		match self {
			Self::Spot => "wss://stream.mexc.com/ws",
			Self::SpotTest => "wss://stream-testnet.mexc.com/ws",
			Self::Futures => "wss://contract.mexc.com/ws",
			Self::None => "",
		}
	}
}
impl WsOption for MexcOption {
	type WsHandler = MexcWsHandler;

	#[inline(always)]
	fn ws_handler(options: Self::Options) -> Self::WsHandler {
		MexcWsHandler::new(options)
	}
}
//,}}}

impl HandlerOptions for MexcOptions {
	type OptionItem = MexcOption;

	fn update(&mut self, option: Self::OptionItem) {
		match option {
			MexcOption::Default => (),
			MexcOption::Pubkey(v) => self.pubkey = Some(v),
			MexcOption::Secret(v) => self.secret = Some(v),
			MexcOption::HttpUrl(v) => self.http_url = v,
			MexcOption::HttpAuth(v) => self.http_auth = v,
			MexcOption::RecvWindow(v) =>
				if v > MAX_RECV_WINDOW {
					tracing::warn!("recvWindow is too large, overwriting with maximum value of {MAX_RECV_WINDOW}");
					self.recv_window = Some(MAX_RECV_WINDOW);
				} else {
					self.recv_window = Some(v);
				},
			MexcOption::WsUrl(v) => self.ws_url = v,
			MexcOption::WsConfig(v) => self.ws_config = v,
		}
	}

	fn is_authenticated(&self) -> bool {
		self.pubkey.is_some() // some end points are satisfied with just the key, and it's really difficult to provide only a key without a secret from the clientside, so assume intent if it's missing.
	}
}

impl<'a, R, B> HttpOption<'a, R, B> for MexcOption
where
	R: DeserializeOwned + 'a,
	B: Serialize,
{
	type RequestHandler = MexcRequestHandler<'a, R>;

	#[inline(always)]
	fn request_handler(options: Self::Options) -> Self::RequestHandler {
		MexcRequestHandler::<'a, R> { options, _phantom: PhantomData }
	}
}

impl HandlerOption for MexcOption {
	type Options = MexcOptions;
}
