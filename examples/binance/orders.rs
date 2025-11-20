use std::{env, time::Duration};

use v_exchanges::{Binance, binance::perp::account::IncomeRequest, prelude::*};

#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let mut binance = Binance::default();

	if let (Ok(key), Ok(secret)) = (env::var("BINANCE_TIGER_READ_PUBKEY"), env::var("BINANCE_TIGER_READ_SECRET")) {
		binance.auth(key, secret.into());

		// Example 1: Query income history
		println!("=== Querying Income History ===");
		let income_req = IncomeRequest {
			symbol: None,
			income_type: None,
			start_time: None,
			end_time: None,
			limit: Some(10),
			page: None,
		};

		match v_exchanges::binance::perp::account::income_history(&binance, income_req, Some(Duration::from_millis(5000))).await {
			Ok(records) => {
				println!("Found {} income records:", records.len());
				for record in records.iter().take(5) {
					println!("  {} - {} {} ({})", record.income_type, record.income, record.asset, record.symbol);
				}
			}
			Err(e) => eprintln!("Error querying income: {}", e),
		}

		// Example 2: Place a limit order (COMMENTED OUT FOR SAFETY)
		// Uncomment and modify the parameters below to actually place an order
		/*
		println!("\n=== Placing Limit Order ===");
		let order_req = OrderRequest {
			symbol: "BTCUSDT".to_string(),
			side: Side::Buy,
			order_type: OrderType::Limit,
			position_side: Some(PositionSide::Both),
			time_in_force: Some(TimeInForce::Gtc),
			qty: Some(0.001),
			price: Some(30000.0),
			stop_price: None,
			reduce_only: None,
			close_position: None,
			activation_price: None,
			callback_rate: None,
			working_type: None,
			price_protect: None,
			new_client_order_id: None,
		};

		match v_exchanges::binance::perp::account::place_order(&binance, order_req, Some(Duration::from_millis(5000))).await {
			Ok(response) => {
				println!("Order placed successfully!");
				println!("  Order ID: {}", response.order_id);
				println!("  Status: {}", response.status);
				println!("  Symbol: {}", response.symbol);
				println!("  Side: {}", response.side);
			}
			Err(e) => eprintln!("Error placing order: {}", e),
		}
		*/

		println!("\n✅ Examples completed (order placement code is commented out for safety)");
	} else {
		eprintln!("BINANCE_TIGER_READ_PUBKEY or BINANCE_TIGER_READ_SECRET is missing");
	}
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
