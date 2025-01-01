use color_eyre::eyre::Result;

use crate::core::AssetBalance;

pub async fn balances(client: &v_exchanges_adapters::Client) -> Result<Vec<AssetBalance>> {
	println!("bybit::account::balances");
	todo!();
}
