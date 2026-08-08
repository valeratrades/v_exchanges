#![feature(duration_constructors)]
use exchange_interactions::{Exchange as _, RetryConfig, binance::Binance};

/// things in here are not on [Exchange](exchange_interactions::core::Exchange) trait, so can't use generics, must specify exact exchange client methods are referenced from.
#[tokio::main]
async fn main() {
	v_utils::clientside!();

	let duration = std::time::Duration::from_mins(2 * 5);
	let bvol = exchange_interactions::bitmex::bvol(duration).await.unwrap();
	dbg!(&bvol);

	let mut bn = Binance::default();
	bn.set_retry_config(RetryConfig {
		max_retries: 3,
		..Default::default()
	});
	let lsrs = bn.lsr(("BTC", "USDT").into(), "5m".into(), (24 * 12 + 1).into(), "Global".into()).await.unwrap();
	dbg!(&lsrs[..2]);

	let vix = exchange_interactions::yahoo::vix_change("1h".into(), 24).await.unwrap();
	dbg!(&vix);
}

#[cfg(test)]
#[test]
fn test_main() {
	main();
}
