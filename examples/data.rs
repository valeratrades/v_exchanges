#![feature(duration_constructors)]
use v_exchanges::{Exchange as _, binance::Binance};

/// things in here are not on [Exchange](v_exchanges::core::Exchange) trait, so can't use generics, must specify exact exchange client methods are referenced from.
#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let duration = std::time::Duration::from_mins(2 * 5);
	let bvol = v_exchanges::bitmex::bvol(duration).await.unwrap();
	dbg!(&bvol);

	let mut bn = Binance::default();
	bn.set_max_tries(3);
	let lsrs = bn.lsr(("BTC", "USDT").into(), "5m".into(), (24 * 12 + 1).into(), "Global".into()).await.unwrap();
	dbg!(&lsrs[..2]);

	let vix = v_exchanges::yahoo::vix_change("1h".into(), 24).await.unwrap();
	dbg!(&vix);
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
