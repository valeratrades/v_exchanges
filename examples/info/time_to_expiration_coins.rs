use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use v_exchanges::{
	Binance, Bybit,
	adapters::{
		binance::{BinanceHttpUrl, BinanceOption},
		bybit::BybitOption,
	},
	binance::perp::general::BinanceExchangeFutures,
};

// Binance perpetuals use this sentinel delivery date (year 2100)
const BINANCE_PERPETUAL_DELIVERY_DATE: i64 = 4133404800000;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BybitInstrumentsResponse {
	result: BybitInstrumentsResult,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BybitInstrumentsResult {
	list: Vec<BybitInstrumentInfo>,
}
#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BybitInstrumentInfo {
	symbol: String,
	contract_type: String,
	#[serde_as(as = "DisplayFromStr")]
	delivery_time: i64,
}

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let binance = Binance::default();
	let bybit = Bybit::default();

	let binance_info: BinanceExchangeFutures = binance
		.get_no_query("/fapi/v1/exchangeInfo", vec![BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM)])
		.await
		.unwrap();

	let bybit_response: BybitInstrumentsResponse = bybit
		.get("/v5/market/instruments-info", &[("category", "linear"), ("limit", "1000")], vec![BybitOption::None])
		.await
		.unwrap();

	let now_ms = Timestamp::now().as_millisecond();

	println!("Binance futures expiration:");
	let mut binance_expiring: Vec<_> = binance_info
		.symbols
		.iter()
		.filter(|s| s.delivery_date != BINANCE_PERPETUAL_DELIVERY_DATE && s.delivery_date > now_ms)
		.collect();
	binance_expiring.sort_by_key(|s| s.delivery_date);
	for symbol in &binance_expiring {
		let remaining_ms = symbol.delivery_date - now_ms;
		let remaining_days = remaining_ms / (1000 * 60 * 60 * 24);
		let remaining_hours = (remaining_ms % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60);
		println!("  {}: {}d {}h", symbol.symbol, remaining_days, remaining_hours);
	}
	if binance_expiring.is_empty() {
		println!("  (none)");
	}

	println!("\nBybit futures expiration:");
	let mut bybit_expiring: Vec<_> = bybit_response
		.result
		.list
		.iter()
		.filter(|i| i.contract_type == "LinearFutures" && i.delivery_time > now_ms)
		.collect();
	bybit_expiring.sort_by_key(|i| i.delivery_time);
	for instrument in &bybit_expiring {
		let remaining_ms = instrument.delivery_time - now_ms;
		let remaining_days = remaining_ms / (1000 * 60 * 60 * 24);
		let remaining_hours = (remaining_ms % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60);
		println!("  {}: {}d {}h", instrument.symbol, remaining_days, remaining_hours);
	}
	if bybit_expiring.is_empty() {
		println!("  (none)");
	}
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
