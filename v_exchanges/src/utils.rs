use v_utils::prelude_libside::*;

/// # Panics
/// Fine, because given prospected usages, theoretically only developer will see it.
pub fn join_params(a: Value, b: Value) -> Value {
	if let (Value::Object(mut a_map), Value::Object(b_map)) = (a, b) {
		a_map.extend(b_map);
		Value::Object(a_map)
	} else {
		panic!("Both inputs must be JSON objects");
	}
}

pub fn usd_value(underlying: f64, asset: Asset, prices: &BTreeMap<Pair, f64>) -> Result<Usd> {
	if underlying == 0. {
		return Ok(Usd(0.));
	}
	if asset == "USDT" {
		return Ok(Usd(underlying));
	}
	let usdt_pair = Pair::new(asset, "USDT".into());
	let usdt_price = prices.get(&usdt_pair).ok_or_else(|| eyre!("No usdt price found for {asset}, which has non-zero balance."))?;
	Ok((underlying * usdt_price).into())
}
