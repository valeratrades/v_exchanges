#![warn(future_incompatible, let_underscore, nonstandard_style)] //, missing_docs)]
#![feature(slice_pattern)]
#![feature(default_field_values)]
#![feature(try_blocks)]
#![feature(duration_constructors)]
#![allow(clippy::result_large_err)]

//! # Generic-API-Client
//! This is a crate for interacting with HTTP/HTTPS/WebSocket APIs.
//! It is named "generic" because you can use the **same** client to interact with **multiple different**
//! APIs with, different authentication methods, data formats etc.
//!
//! This crate  provides
//! - [Client][http::Client] A HTTP/HTTPS client
//! - [WebSocketConnection][websocket::WebSocketConnection] A `struct` to manage WebSocket connections
//! - [RequestHandler][http::RequestHandler] A `trait` for implementing features like authentication on your requests
//! - [WebSocketHandler][websocket::WebSocketHandler] A `trait` that is used to handle messages etc.. for a WebSocket Connection.
//!
//! For a more detailed documentation, see the links above.

pub mod http;
pub mod ws;
pub extern crate reqwest;
pub extern crate tokio_tungstenite;

#[derive(Debug, miette::Diagnostic, derive_more::Display, thiserror::Error, derive_more::From)]
#[non_exhaustive]
pub enum AuthError {
	#[diagnostic(code(v_exchanges::auth::missing_pubkey), help("Provide API public key in your credentials"))]
	MissingPubkey,
	#[diagnostic(code(v_exchanges::auth::missing_secret), help("Provide API secret key in your credentials"))]
	MissingSecret,
	#[diagnostic(code(v_exchanges::auth::invalid_api_key))]
	InvalidCharacterInApiKey(String),
	#[diagnostic(code(v_exchanges::auth::other))]
	Other(eyre::Report),
}

#[derive(Debug, miette::Diagnostic, thiserror::Error)]
pub enum UrlError {
	#[error("Failed to parse URL: {0}")]
	#[diagnostic(code(v_exchanges::url::parse))]
	Parse(#[from] url::ParseError),
	#[error("Exchange does not provide testnet for requested endpoint: {0}")]
	#[diagnostic(
		code(v_exchanges::url::missing_testnet),
		help("Not all exchanges provide testnet endpoints. Check if this exchange supports testnet for this specific API.")
	)]
	MissingTestnet(url::Url),
}
