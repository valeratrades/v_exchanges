#![warn(
	future_incompatible,
	let_underscore,
	nonstandard_style,
	missing_docs
)]

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

/// Module for interacting with HTTP/HTTPS APIs.
pub mod http;
/// Module for interacting with WebSocket APIs.
pub mod websocket;
