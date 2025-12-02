#![cfg_attr(docsrs, feature(doc_cfg))]
#![feature(default_field_values)]
#![feature(duration_constructors)]
#![feature(try_blocks)]
pub extern crate v_exchanges_api_generics as generics;
use std::sync::Arc;

pub use exchanges::*;
use generics::UrlError;
use serde::Serialize;
use tokio::sync::Semaphore;
use traits::*;
use v_exchanges_api_generics::{
	http::{self, *},
	ws::*,
};

mod exchanges;
pub mod traits;

// very long type, make it a macro
macro_rules! request_ret {
    ($lt:lifetime, $Response:ty, $Options:ty,  $Body:ty) => {
        Result<
            <<$Options as HttpOption<$lt, $Response, $Body>>::RequestHandler as RequestHandler<$Body>>::Successful,
            RequestError,
        >
    };
}

/// Default maximum number of simultaneous requests allowed
pub const DEFAULT_MAX_SIMULTANEOUS_REQUESTS: usize = 100;

#[derive(Clone, Debug)]
pub struct Client {
	pub client: http::Client,
	/// Semaphore for limiting simultaneous requests.
	/// Shared across clones of this client.
	pub request_semaphore: Arc<Semaphore>,
	#[cfg(feature = "binance")]
	binance: binance::BinanceOptions,
	#[cfg(feature = "bitflyer")]
	bitflyer: bitflyer::BitFlyerOptions,
	#[cfg(feature = "bybit")]
	bybit: bybit::BybitOptions,
	#[cfg(feature = "coincheck")]
	coincheck: coincheck::CoincheckOptions,
	#[cfg(feature = "kucoin")]
	kucoin: kucoin::KucoinOptions,
	#[cfg(feature = "mexc")]
	mexc: mexc::MexcOptions,
}

impl Default for Client {
	fn default() -> Self {
		Self {
			client: http::Client::default(),
			request_semaphore: Arc::new(Semaphore::new(DEFAULT_MAX_SIMULTANEOUS_REQUESTS)),
			#[cfg(feature = "binance")]
			binance: binance::BinanceOptions::default(),
			#[cfg(feature = "bitflyer")]
			bitflyer: bitflyer::BitFlyerOptions::default(),
			#[cfg(feature = "bybit")]
			bybit: bybit::BybitOptions::default(),
			#[cfg(feature = "coincheck")]
			coincheck: coincheck::CoincheckOptions::default(),
			#[cfg(feature = "kucoin")]
			kucoin: kucoin::KucoinOptions::default(),
			#[cfg(feature = "mexc")]
			mexc: mexc::MexcOptions::default(),
		}
	}
}

impl Client {
	/// Set the maximum number of simultaneous requests allowed.
	///
	/// This creates a new semaphore with the specified number of permits.
	/// Note: This will NOT affect existing clones of this client - they will keep using the old semaphore.
	/// Call this before cloning if you need all instances to share the same limit.
	pub fn set_max_simultaneous_requests(&mut self, max: usize) {
		self.request_semaphore = Arc::new(Semaphore::new(max));
	}

	/// Update the default options for this [Client]
	pub fn update_default_option<O>(&mut self, option: O)
	where
		O: HandlerOption,
		Self: GetOptions<O::Options>, {
		self.default_options_mut().update(option);
	}

	pub fn is_authenticated<O>(&self) -> bool
	where
		O: HandlerOption,
		Self: GetOptions<O::Options>, {
		self.default_options().is_authenticated()
	}

	#[inline]
	fn merged_options<O>(&self, options: impl IntoIterator<Item = O>) -> O::Options
	where
		O: HandlerOption,
		Self: GetOptions<O::Options>, {
		let mut default_options = self.default_options().clone();
		for option in options {
			default_options.update(option);
		}
		default_options
	}

	/// see [http::Client::request()]
	pub async fn request<'a, R, O, Q, B>(&self, method: Method, url: &str, query: Option<&Q>, body: Option<B>, options: impl IntoIterator<Item = O>) -> request_ret!('a, R, O, B)
	where
		O: HttpOption<'a, R, B>,
		O::RequestHandler: RequestHandler<B>,
		Self: GetOptions<O::Options>,
		Q: Serialize + ?Sized + std::fmt::Debug, {
		self.client.request(method, url, query, body, &O::request_handler(self.merged_options(options))).await
	}

	/// see [http::Client::get()]
	pub async fn get<'a, R, O, Q>(&self, url: &str, query: &Q, options: impl IntoIterator<Item = O>) -> request_ret!('a, R, O, ())
	where
		O: HttpOption<'a, R, ()>,
		O::RequestHandler: RequestHandler<()>,
		Self: GetOptions<O::Options>,
		Q: Serialize + ?Sized + std::fmt::Debug, {
		self.client.get(url, query, &O::request_handler(self.merged_options(options))).await
	}

	/// see [http::Client::get_no_query()]
	pub async fn get_no_query<'a, R, O>(&self, url: &str, options: impl IntoIterator<Item = O>) -> request_ret!('a, R, O, ())
	where
		O: HttpOption<'a, R, ()>,
		O::RequestHandler: RequestHandler<()>,
		Self: GetOptions<O::Options>, {
		self.client.get_no_query(url, &O::request_handler(self.merged_options(options))).await
	}

	/// see [http::Client::post()]
	pub async fn post<'a, R, O, B>(&self, url: &str, body: B, options: impl IntoIterator<Item = O>) -> request_ret!('a, R, O, B)
	where
		O: HttpOption<'a, R, B>,
		O::RequestHandler: RequestHandler<B>,
		Self: GetOptions<O::Options>, {
		self.client.post(url, body, &O::request_handler(self.merged_options(options))).await
	}

	/// see [http::Client::post_no_body()]
	pub async fn post_no_body<'a, R, O>(&self, url: &str, options: impl IntoIterator<Item = O>) -> request_ret!('a, R, O, ())
	where
		O: HttpOption<'a, R, ()>,
		O::RequestHandler: RequestHandler<()>,
		Self: GetOptions<O::Options>, {
		self.client.post_no_body(url, &O::request_handler(self.merged_options(options))).await
	}

	/// see [http::Client::put()]
	pub async fn put<'a, R, O, B>(&self, url: &str, body: B, options: impl IntoIterator<Item = O>) -> request_ret!('a, R, O, B)
	where
		O: HttpOption<'a, R, B>,
		O::RequestHandler: RequestHandler<B>,
		Self: GetOptions<O::Options>, {
		self.client.put(url, body, &O::request_handler(self.merged_options(options))).await
	}

	/// see [http::Client::put_no_body()]
	pub async fn put_no_body<'a, R, O>(&self, url: &str, options: impl IntoIterator<Item = O>) -> request_ret!('a, R, O, ())
	where
		O: HttpOption<'a, R, ()>,
		O::RequestHandler: RequestHandler<()>,
		Self: GetOptions<O::Options>, {
		self.client.put_no_body(url, &O::request_handler(self.merged_options(options))).await
	}

	/// see [http::Client::delete()]
	pub async fn delete<'a, R, O, Q>(&self, url: &str, query: &Q, options: impl IntoIterator<Item = O>) -> request_ret!('a, R, O, ())
	where
		O: HttpOption<'a, R, ()>,
		O::RequestHandler: RequestHandler<()>,
		Self: GetOptions<O::Options>,
		Q: Serialize + ?Sized + std::fmt::Debug, {
		self.client.delete(url, query, &O::request_handler(self.merged_options(options))).await
	}

	/// see [http::Client::delete_no_query()]
	pub async fn delete_no_query<'a, R, O>(&self, url: &str, options: impl IntoIterator<Item = O>) -> request_ret!('a, R, O, ())
	where
		O: HttpOption<'a, R, ()>,
		O::RequestHandler: RequestHandler<()>,
		Self: GetOptions<O::Options>, {
		self.client.delete_no_query(url, &O::request_handler(self.merged_options(options))).await
	}

	pub fn ws_connection<O>(&self, url: &str, options: impl IntoIterator<Item = O>) -> Result<WsConnection<O::WsHandler>, UrlError>
	where
		O: WsOption,
		O::WsHandler: WsHandler,
		Self: GetOptions<O::Options>, {
		WsConnection::try_new(url, O::ws_handler(self.merged_options(options)))
	}
}

pub trait GetOptions<O: HandlerOptions> {
	fn default_options(&self) -> &O;
	fn default_options_mut(&mut self) -> &mut O;
	fn is_authenticated(&self) -> bool {
		self.default_options().is_authenticated()
	}
}

#[cfg(feature = "binance")]
#[cfg_attr(docsrs, doc(cfg(feature = "binance")))]
impl GetOptions<binance::BinanceOptions> for Client {
	fn default_options(&self) -> &binance::BinanceOptions {
		&self.binance
	}

	fn default_options_mut(&mut self) -> &mut binance::BinanceOptions {
		&mut self.binance
	}
}

#[cfg(feature = "bitflyer")]
#[cfg_attr(docsrs, doc(cfg(feature = "bitflyer")))]
impl GetOptions<bitflyer::BitFlyerOptions> for Client {
	fn default_options(&self) -> &bitflyer::BitFlyerOptions {
		&self.bitflyer
	}

	fn default_options_mut(&mut self) -> &mut bitflyer::BitFlyerOptions {
		&mut self.bitflyer
	}
}

#[cfg(feature = "bybit")]
#[cfg_attr(docsrs, doc(cfg(feature = "bybit")))]
impl GetOptions<bybit::BybitOptions> for Client {
	fn default_options(&self) -> &bybit::BybitOptions {
		&self.bybit
	}

	fn default_options_mut(&mut self) -> &mut bybit::BybitOptions {
		&mut self.bybit
	}
}

#[cfg(feature = "coincheck")]
#[cfg_attr(docsrs, doc(cfg(feature = "coincheck")))]
impl GetOptions<coincheck::CoincheckOptions> for Client {
	fn default_options(&self) -> &coincheck::CoincheckOptions {
		&self.coincheck
	}

	fn default_options_mut(&mut self) -> &mut coincheck::CoincheckOptions {
		&mut self.coincheck
	}
}
#[cfg(feature = "kucoin")]
#[cfg_attr(docsrs, doc(cfg(feature = "kucoin")))]
impl GetOptions<kucoin::KucoinOptions> for Client {
	fn default_options(&self) -> &kucoin::KucoinOptions {
		&self.kucoin
	}

	fn default_options_mut(&mut self) -> &mut kucoin::KucoinOptions {
		&mut self.kucoin
	}
}
#[cfg(feature = "mexc")]
#[cfg_attr(docsrs, doc(cfg(feature = "mexc")))]
impl GetOptions<mexc::MexcOptions> for Client {
	fn default_options(&self) -> &mexc::MexcOptions {
		&self.mexc
	}

	fn default_options_mut(&mut self) -> &mut mexc::MexcOptions {
		&mut self.mexc
	}
}
