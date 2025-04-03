use adapters::{
	binance::{BinanceOption, BinanceWsUrl},
	generics::ws::WsConnection,
};
use chrono::{DateTime, Utc};
use serde_with::{DisplayFromStr, TimestampSeconds, serde_as};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite;
use v_utils::trades::{Pair, Side};

use super::Binance;
use crate::{AbsMarket, ExchangeResult, WrongExchangeError, ws_types::TradeEvent};

impl Binance {
	pub async fn ws_trade_futs(&self, pair: Pair) -> ExchangeResult<mpsc::Receiver<Result<TradeEvent, tungstenite::Error>>> {
		let topic = format!("ws/{}@trade", pair.fmt_binance().to_lowercase());
		let mut connection = self.ws_connection(&topic, vec![BinanceOption::WsUrl(BinanceWsUrl::FuturesUsdM)]);
		dbg!(&connection, &connection.url.as_str());

		let (tx, rx) = mpsc::channel::<Result<TradeEvent, tungstenite::Error>>(256);

		tokio::spawn(async move {
			loop {
				let resp = connection.next().await;
				static EXPECT_REASON: &str = "Fails if either a) exchange changed trade_event's serialization (unrecoverable), either b) exchange-communication layer failed to pick out an error response, which means we probably shouldn't run in production yet.\n";
				let result_trade_event = resp.map(|msg| {
					assert_eq!(msg["e"], "trade", "{EXPECT_REASON}");
					let initial = serde_json::from_value::<TradeEventFuts>(msg).expect(EXPECT_REASON);
					TradeEvent::from(initial)
				});

				if tx.send(result_trade_event).await.is_err() {
					tracing::debug!("Receiver dropped, dropping the connection");
					break;
				}
			}
		});

		Ok(rx)
	}
}

impl From<TradeEventFuts> for TradeEvent {
	fn from(futs: TradeEventFuts) -> Self {
		Self {
			time: DateTime::from_timestamp_millis(futs.time).expect("Exchange responded with invalid timestamp"),
			qty_asset: futs.qty_asset,
			price: futs.price,
		}
	}
}

#[serde_as]
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct TradeEventFuts {
	#[serde(rename = "E")]
	_idk_what_this_is: serde_json::Value,
	#[serde(rename = "T")]
	time: i64,
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
