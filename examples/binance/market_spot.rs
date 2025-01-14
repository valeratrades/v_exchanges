use v_exchanges::prelude::*;

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));

	let m: AbsMarket = "Binance/Spot".into();
	let c = m.client();

	let spot_klines = c.klines(("BTC", "USDT").into(), "1m".into(), 2.into(), m).await.unwrap();
	dbg!(&spot_klines);

	let spot_prices = c.prices(None, m).await.unwrap();
	dbg!(&spot_prices[..5]);
}
