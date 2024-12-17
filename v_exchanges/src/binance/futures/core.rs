use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};

//TODO: make these actually consistent

/** # Ex: ```json
[1731448080000,\"88591.90\",\"88630.90\",\"88560.00\",\"88574.10\",\"173.581\",1731448139999,\"15378315.48720\",2800,\"113.654\",\"10069629.84420\",\"0\"]
```
**/
#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct KlineCore {
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
	pub volume: f64,

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

	#[serde_as(as = "DisplayFromStr")]
	#[serde(skip, rename = "B")]
	pub __ignore: Option<f64>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct KlineEvent {
	#[serde(rename = "e")]
	pub event_type: String,

	#[serde(rename = "E")]
	pub event_time: u64,

	#[serde(rename = "s")]
	pub symbol: String,

	#[serde(rename = "k")]
	pub kline: Kline,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Kline {
	#[serde(flatten)]
	pub core: KlineCore,

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
