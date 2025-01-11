use v_exchanges::{
	binance,
	core::{Exchange, MarketTrait as _},
};

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));

	//let m: Market = "Binance/Spot".into(); // would be nice to be able to do it like this, without having to carry around exchange-specific type
	let m = binance::Market::Spot;
	let bn = m.client();

	let spot_klines = bn.klines(("BTC", "USDT").into(), "1m".into(), 2.into(), m).await.unwrap();
	dbg!(&spot_klines);

	let spot_prices = bn.prices(None, m).await.unwrap();
	dbg!(&spot_prices[..5]);
}
