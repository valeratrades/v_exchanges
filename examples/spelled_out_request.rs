use v_exchanges_adapters::{Client, bybit::BybitOption};

#[tokio::main]
async fn main() {
	let client = Client::default();
	let r: serde_json::Value = client
		.get(
			"/v5/market/account-ratio",
			&[("category", "linear"), ("symbol", "BTCUSDT"), ("period", "1h"), ("limit", "50")], //TODO!!!!!!: add this `5min` repr thing to Bybit tf type
			[BybitOption::None],
		)
		.await
		.expect("failed to get ticker");
	println!("{r:#?}");
}
