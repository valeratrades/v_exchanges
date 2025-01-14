use v_exchanges::{binance::Binance, bitmex::Bitmex};

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));

	let bn = Binance::default();
	let lsr = bn.lsr(("BTC", "USDT").into(), "5m".into(), (24 * 12 + 1).into(), "Global".into()).await.unwrap();
	dbg!(&lsr[..2]);
}
