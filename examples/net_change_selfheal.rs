//! Behavioral repro for the HTTP self-healing connection pool on host network change.
//!
//! Loops calling `open_interest` every few seconds, logging ok/err. While it runs, force a
//! network-identity change WITHOUT a real suspend:
//!   sudo ip addr add 10.123.0.1/32 dev <iface>
//!   sudo ip addr del 10.123.0.1/32 dev <iface>
//! Expect: a "host network change observed; rebuilt HTTP connection pool" log line, and the
//! OKs continue uninterrupted. For the genuine case, `systemctl suspend` then resume.
use std::{str::FromStr as _, time::Duration};

use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let binance = ExchangeName::Binance.init_client();
	let symbol = Symbol::from_str("BTC-USDT.P").unwrap();

	loop {
		match binance.open_interest(symbol, "1h".into(), 5.into()).await {
			Ok(oi) => tracing::info!(?oi, "ok"),
			Err(e) => tracing::error!(?e, "err"),
		}
		tokio::time::sleep(Duration::from_secs(3)).await;
	}
}
