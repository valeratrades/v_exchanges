#![feature(default_field_values)]
//! Local market-data persistence for v_exchanges.
//!
//! Two-tier: hot [`Feather`](feather::Feather) buffers (Arrow IPC) rotate into a cold
//! [`Catalog`](catalog::Catalog) of immutable Parquet files. Replay merges the lanes into a
//! deterministic ordered stream.
//!
//! # Lanes
//!
//! Five fixed lanes, one per [`Data`](schema::Data) variant: `snapshots`, `deltas`, `trades`,
//! `closes`, `custom`. Each lane has its own subdirectory tree and rotation policy.
//!
//! See module docs for details.

pub mod book_persistor;
pub mod catalog;
pub mod clock;
pub mod feather;
pub mod replay;
pub mod schema;

pub use book_persistor::CatalogBookPersistor;
pub use catalog::{Catalog, Lane};
pub use clock::{Clock, LiveClock};
pub use feather::{Feather, RotationPolicy};
pub use replay::{ReplayConfig, replay};
pub use schema::{BookDelta, BookSnapshot, Close, Custom, Data, Trade, UnixNanos};
