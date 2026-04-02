use std::{collections::BTreeMap, env};

use jiff::Timestamp;
use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	// (label, exchange, instrument)
	let configs: &[(&str, &str, Instrument)] = &[("BINANCE_TIGER_READ", "binance", Instrument::Perp), ("QUANTM_BYBIT_SUB", "bybit", Instrument::Perp)];

	// group expire_time per exchange name
	let mut by_exchange: BTreeMap<&str, Vec<(&str, Option<Timestamp>)>> = BTreeMap::new();

	for (prefix, exchange_label, instrument) in configs {
		let pubkey_var = format!("{prefix}_PUBKEY");
		let secret_var = format!("{prefix}_SECRET");

		let (Ok(pubkey), Ok(secret)) = (env::var(&pubkey_var), env::var(&secret_var)) else {
			eprintln!("{pubkey_var} or {secret_var} not set, skipping.");
			continue;
		};

		let mut client = match *exchange_label {
			"binance" => ExchangeName::Binance.init_client(),
			"bybit" => ExchangeName::Bybit.init_client(),
			other => panic!("unknown exchange: {other}"),
		};
		client.auth(pubkey, secret.into());

		match client.personal_info(*instrument, None).await {
			Ok(info) => {
				by_exchange.entry(exchange_label).or_default().push((prefix, info.api.expire_time));
			}
			Err(e) => eprintln!("{prefix}: failed to fetch personal info - {e}"),
		}
	}

	let now = Timestamp::now();

	for (exchange, keys) in &by_exchange {
		println!("{exchange}:");
		for (prefix, expire_time) in keys {
			match expire_time {
				None => println!("  {prefix}: never expires"),
				Some(t) => {
					let remaining = *t - now;
					let total_secs = remaining.get_seconds();
					let days = total_secs / 86400;
					let hours = (total_secs % 86400) / 3600;
					println!("  {prefix}: expires in {days}d {hours}h  (at {t})");
				}
			}
		}
	}
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
