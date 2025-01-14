use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));

	//let m: Market = "Binance/Spot".into(); // would be nice to be able to do it like this, without having to carry around exchange-specific type
	// Currently if I want to pass around the market struct in my code after initializing it, I have to pass around eg `binance::Market`, which is a ridiculous thing to hardcode into function signatures
	//let m = binance::Market::Spot;
	let m: AbsMarket = "Binance/Spot".into();
	let c = m.client();

	let spot_klines = c.klines(("BTC", "USDT").into(), "1m".into(), 2.into(), m).await.unwrap();
	dbg!(&spot_klines);

	let spot_prices = c.prices(None, m).await.unwrap();
	dbg!(&spot_prices[..5]);
}
