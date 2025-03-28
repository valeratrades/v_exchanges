#![warn(future_incompatible, let_underscore, nonstandard_style, missing_docs)]
#![feature(slice_pattern)]
#![feature(default_field_values)]
#![feature(try_blocks)]
#![feature(duration_constructors)]
#![feature(let_chains)]

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
#[deprecated(note = "switching to `ws`")]
pub mod websocket;
pub mod ws;
pub extern crate reqwest;
