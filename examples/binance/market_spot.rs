use v_exchanges::{binance::Binance, core::Exchange};

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));
	let bn = Binance::default();

	let spot_klines = bn.spot_klines(("BTC", "USDT").into(), "1m".into(), 2.into()).await.unwrap();
	dbg!(&spot_klines);

	let spot_prices = bn.spot_prices(None).await.unwrap();
	dbg!(&spot_prices[..5]);
}
