use v_exchanges::{adapters::binance::BinanceOption, binance::Binance, bitmex::Bitmex};

/// things in here are not on [Exchange](v_exchanges::core::Exchange) trait, so can't use generics, must specify exact exchange client methods are referenced from.
#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let bm = Bitmex::default();
	let bvol = bm.bvol(2).await.unwrap();
	dbg!(&bvol);

	let bn = Binance::default();
	let lsrs = bn.lsr(("BTC", "USDT").into(), "5m".into(), (24 * 12 + 1).into(), "Global".into()).await.unwrap();
	dbg!(&lsrs[..2]);
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
