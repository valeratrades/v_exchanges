use v_exchanges::{binance::Binance, bitmex::Bitmex};

/// things in here are not on [Exchange](v_exchanges::core::Exchange) trait, so can't use generics, must specify exact exchange client methods are referenced from.
#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));
	let bm = Bitmex::default();
	let bvol = bm.bvol(2).await.unwrap();
	dbg!(&bvol);

	let bn = Binance::default();
	let lsr = bn.lsr(("BTC", "USDT").into(), "5m".into(), (24 * 12 + 1).into(), "Global".into()).await.unwrap();
	dbg!(&lsr[..2]);
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
