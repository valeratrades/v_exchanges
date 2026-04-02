use jiff::Timestamp;
use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let now = Timestamp::now();

	for (name, exchange) in [("Binance", ExchangeName::Binance.init_client()), ("Bybit", ExchangeName::Bybit.init_client())] {
		let info = exchange.exchange_info(Instrument::Perp).await.unwrap();
		let mut expiring: Vec<_> = info.pairs.iter().filter_map(|(pair, pair_info)| pair_info.delivery_date.map(|d| (pair, d))).collect();
		expiring.sort_by_key(|(_, d)| *d);

		println!("{name} futures expiration:");
		if expiring.is_empty() {
			println!("  (none)");
		}
		for (pair, delivery_date) in expiring {
			let remaining = delivery_date - now;
			let total_secs = remaining.get_seconds();
			let days = total_secs / 86400;
			let hours = (total_secs % 86400) / 3600;
			println!("  {pair}: {days}d {hours}h");
		}
	}
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
