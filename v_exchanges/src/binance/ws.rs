use adapters::{
	Client,
	generics::ws::WsError,
};
use chrono::DateTime;
use serde_with::{DisplayFromStr, serde_as};
use tokio::sync::mpsc;

use crate::{Symbol, TradeEvent};

// trades {{{
//TODO!!!!!!!: switch to implementing the ExchangeStream trait
pub async fn trades(client: &Client, symbol: Symbol) -> mpsc::Receiver<Result<TradeEvent, WsError>> {
	todo!();
	//let topic = format!("ws/{}@trade", pair.fmt_binance().to_lowercase());
	//let base_url = match m {
	//	Market::Perp => BinanceWsUrl::FuturesUsdM,
	//	Market::Spot | Market::Marg => BinanceWsUrl::Spot,
	//	_ => unimplemented!(),
	//};
	//let mut connection = client.ws_connection(&topic, vec![BinanceOption::WsUrl(base_url)]);
	//let (tx, rx) = mpsc::channel::<Result<TradeEvent, WsError>>(256);
	//
	////SPAWN: can go around it with proper impl of futures_core::Stream, but that would require "unwrapping" async into the underlying state-machine on Poll in every instance of its utilization there
	//tokio::spawn(async move {
	//	loop {
	//		let resp = connection.next().await;
	//		static EXPECT_REASON: &str = "Fails if either a) exchange changed trade_event's serialization (unrecoverable), either b) exchange-communication layer failed to pick out an error response, which means we probably shouldn't run in production yet.\n";
	//
	//		let result_trade_event = resp.map(|msg| {
	//			assert_eq!(msg["e"], "trade", "{EXPECT_REASON}");
	//			match m {
	//				Market::Perp => {
	//					let initial = serde_json::from_value::<TradeEventFuts>(msg).expect(EXPECT_REASON);
	//					TradeEvent::from(initial)
	//				}
	//				Market::Spot | Market::Marg => {
	//					let initial = serde_json::from_value::<TradeEventSpot>(msg).expect(EXPECT_REASON);
	//					TradeEvent::from(initial)
	//				}
	//				_ => unimplemented!(),
	//			}
	//		});
	//
	//		if tx.send(result_trade_event).await.is_err() {
	//			tracing::debug!("Receiver dropped, dropping the connection");
	//			break;
	//		}
	//	}
	//});
	//
	//rx
}

#[serde_as]
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct TradeEventFuts {
	#[serde(rename = "T")]
	timestamp: i64,
	#[serde(rename = "X")]
	_order_type: String,
	#[serde(rename = "m")]
	_is_maker: bool,
	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "q")]
	qty_asset: f64,
	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "p")]
	price: f64,
	#[serde(rename = "s")]
	_pair: String,
	#[serde(rename = "t")]
	_trade_id: u64,
}
impl From<TradeEventFuts> for TradeEvent {
	fn from(futs: TradeEventFuts) -> Self {
		Self {
			time: DateTime::from_timestamp_millis(futs.timestamp).expect("Exchange responded with invalid timestamp"),
			qty_asset: futs.qty_asset,
			price: futs.price,
		}
	}
}

#[serde_as]
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct TradeEventSpot {
	#[serde(rename = "T")]
	timestamp: i64,
	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "q")]
	qty_asset: f64,
	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "p")]
	price: f64,
	#[serde(rename = "s")]
	_pair: String,
}
impl From<TradeEventSpot> for TradeEvent {
	fn from(futs: TradeEventSpot) -> Self {
		Self {
			time: DateTime::from_timestamp_millis(futs.timestamp).expect("Exchange responded with invalid timestamp"),
			qty_asset: futs.qty_asset,
			price: futs.price,
		}
	}
}

//,}}}
