use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::{DisplayFromStr, serde_as};

//TODO: make these actually consistent

// // Klines Core
#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KlineNamed {
	#[serde(rename = "t")]
	pub open_time: i64,

	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "o")]
	pub open: f64,

	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "c")]
	pub close: f64,

	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "h")]
	pub high: f64,

	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "l")]
	pub low: f64,

	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "v")]
	pub volume_quote: f64,

	#[serde(rename = "T")]
	pub close_time: i64,

	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "q")]
	pub quote_asset_volume: f64,

	#[serde(rename = "n")]
	pub number_of_trades: i64,

	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "V")]
	pub taker_buy_base_asset_volume: f64,

	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "Q")]
	pub taker_buy_quote_asset_volume: f64,

	#[serde(rename = "B")]
	__ignore: Option<Value>,
}
//

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KlineEvent {
	#[serde(rename = "e")]
	pub event_type: String,

	#[serde(rename = "E")]
	pub event_time: u64,

	#[serde(rename = "s")]
	pub symbol: String,

	#[serde(rename = "k")]
	pub kline: FullKlines,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FullKlines {
	#[serde(flatten)]
	pub core: KlineNamed,

	#[serde(rename = "s")]
	pub symbol: String,

	#[serde(rename = "i")]
	pub interval: String,

	#[serde(rename = "f")]
	pub first_trade_id: i64,

	#[serde(rename = "L")]
	pub last_trade_id: i64,

	#[serde(rename = "x")]
	pub is_final_bar: bool,

	#[serde_as(as = "DisplayFromStr")]
	#[serde(skip, rename = "B")]
	pub __ignore: u64,
}
