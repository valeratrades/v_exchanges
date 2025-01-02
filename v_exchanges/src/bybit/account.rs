use std::str::FromStr;

use color_eyre::eyre::Result;
use serde::{Deserialize, Serialize};

use crate::core::AssetBalance;

pub async fn balances(client: &v_exchanges_adapters::Client) -> Result<Vec<AssetBalance>> {
	println!("bybit::account::balances");
	todo!();
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AccountType {
	Spot,
	Contract,
	Unified,
	Funding,
	Option,
}
impl std::fmt::Display for AccountType {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let s = serde_plain::to_string(self).map_err(|_| std::fmt::Error)?;
		write!(f, "{s}")
	}
}
impl FromStr for AccountType {
	type Err = ();

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		serde_plain::from_str(s).map_err(|_| ())
	}
}

mod tests {
	use insta;

	use super::*;

	#[test]
	fn test_account_type_serde() {
		insta::assert_debug_snapshot!(format!("{}", AccountType::Unified), @r#""UNIFIED""#);
		let s = "UNIFIED";
		let _: AccountType = s.parse().unwrap();
	}
}
