use v_exchanges_adapters::{Client, bybit::BybitOption};

#[tokio::main]
async fn main() {
	let client = Client::default();
	let hype: serde_json::Value = client
		.get(
			"/v5/market/account-ratio",
			&[("category", "linear"), ("symbol", "HYPEUSDC"), ("period", "1h"), ("limit", "50")], //TODO!!!!!!: add this `5min` repr thing to Bybit tf type
			[BybitOption::None],
		)
		.await
		.expect("failed to get ticker");
	println!("{hype:#?}");
}
