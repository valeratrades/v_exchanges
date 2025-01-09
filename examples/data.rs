use v_exchanges::{binance::Binance, bitmex::Bitmex};

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));
	let bm = Bitmex::default();
	let bvol = bm.bvol(2).await.unwrap();
	dbg!(&bvol);

	let bn = Binance::default();
	let lsr = bn.global_lsr_account(("BTC", "USDT").into(), "5m".into(), 24 * 12 + 1, "Global".into()).await.unwrap();
	dbg!(&lsr[..2]);
}
