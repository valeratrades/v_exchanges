use std::{collections::BTreeMap, env};

use jiff::Timestamp;
use v_exchanges::{kucoin::KucoinOption, prelude::*};

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	// (label, exchange, instrument, expire_time)
	let mut by_exchange: BTreeMap<&str, Vec<(String, Option<Timestamp>)>> = BTreeMap::new();

	macro_rules! fetch {
		($exchange:expr, $label:expr, $instrument:expr, $client:expr) => {{
			let label = format!("{} ({:?})", $label, $instrument);
			match $client.personal_info($instrument, None).await {
				Ok(info) => by_exchange.entry($exchange).or_default().push((label, info.api.expire_time)),
				Err(e) => eprintln!("{label}: failed - {e}"),
			}
		}};
	}

	// Binance
	if let (Ok(pub_), Ok(sec)) = (env::var("BINANCE_TIGER_FULL_PUBKEY"), env::var("BINANCE_TIGER_FULL_SECRET")) {
		let mut c = ExchangeName::Binance.init_client();
		c.auth(pub_.clone(), sec.clone().into());
		fetch!("binance", "BINANCE_TIGER_FULL", Instrument::Perp, c);
		let mut c = ExchangeName::Binance.init_client();
		c.auth(pub_, sec.into());
		fetch!("binance", "BINANCE_TIGER_FULL", Instrument::Spot, c);
	} else {
		eprintln!("BINANCE_TIGER_FULL_PUBKEY or BINANCE_TIGER_FULL_SECRET not set, skipping.");
	}

	// Bybit
	if let (Ok(pub_), Ok(sec)) = (env::var("QUANTM_BYBIT_SUB_PUBKEY"), env::var("QUANTM_BYBIT_SUB_SECRET")) {
		let mut c = ExchangeName::Bybit.init_client();
		c.auth(pub_, sec.into());
		fetch!("bybit", "QUANTM_BYBIT_SUB", Instrument::Perp, c);
	} else {
		eprintln!("QUANTM_BYBIT_SUB_PUBKEY or QUANTM_BYBIT_SUB_SECRET not set, skipping.");
	}

	// Kucoin (requires passphrase — can't go through dyn Exchange)
	if let (Ok(pub_), Ok(sec), Ok(pass)) = (env::var("KUCOIN_API_PUBKEY"), env::var("KUCOIN_API_SECRET"), env::var("KUCOIN_API_PASSPHRASE")) {
		let mut c = Kucoin::default();
		c.auth(pub_, sec.into());
		c.update_default_option(KucoinOption::Passphrase(pass.into()));
		fetch!("kucoin", "KUCOIN_API", Instrument::Spot, c);
	} else {
		eprintln!("KUCOIN_API_PUBKEY, KUCOIN_API_SECRET, or KUCOIN_API_PASSPHRASE not set, skipping.");
	}

	let now = Timestamp::now();

	for (exchange, keys) in &by_exchange {
		println!("{exchange}:");
		for (label, expire_time) in keys {
			match expire_time {
				None => println!("  {label}: never expires"),
				Some(t) => {
					let remaining = *t - now;
					let total_secs = remaining.get_seconds();
					let days = total_secs / 86400;
					let hours = (total_secs % 86400) / 3600;
					println!("  {label}: expires in {days}d {hours}h  (at {t})");
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
