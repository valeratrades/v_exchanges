use adapters::{
	Client,
	binance::{BinanceOption, BinanceWsHandler, BinanceWsUrl},
	generics::ws::{WsConnection, WsError},
};
use jiff::Timestamp;
use serde_with::{DisplayFromStr, serde_as};
use v_utils::trades::Pair;

use crate::{ExchangeStream, Instrument, Trade};

// trades {{{
#[derive(Debug, derive_more::Deref, derive_more::DerefMut)]
pub struct TradesConnection {
	#[deref]
	#[deref_mut]
	connection: WsConnection<BinanceWsHandler>,
	instrument: Instrument,
}
impl TradesConnection {
	pub fn new(client: &Client, pairs: Vec<Pair>, instrument: Instrument) -> Result<Self, WsError> {
		let vec_topic_str = pairs.into_iter().map(|p| format!("{}@trade", p.fmt_binance().to_lowercase())).collect::<Vec<_>>();

		let base_url = match instrument {
			Instrument::Perp => BinanceWsUrl::FuturesUsdM,
			Instrument::Spot | Instrument::Margin => BinanceWsUrl::Spot,
			_ => unimplemented!(),
		};
		let connection = client.ws_connection("", vec![BinanceOption::WsUrl(base_url), BinanceOption::WsTopics(vec_topic_str)])?;

		Ok(Self { connection, instrument })
	}
}
#[async_trait::async_trait]
impl ExchangeStream for TradesConnection {
	type Item = Trade;

	async fn next(&mut self) -> Result<Self::Item, WsError> {
		loop {
			let content_event = self.connection.next().await?;
			let trade = match self.instrument {
				Instrument::Perp => {
					let parsed = serde_json::from_value::<TradeEventPerp>(content_event.data.clone()).expect("Exchange responded with invalid trade event");
					Trade::from(parsed)
				}
				Instrument::Spot | Instrument::Margin => {
					let parsed = serde_json::from_value::<TradeEventSpot>(content_event.data.clone()).expect("Exchange responded with invalid trade event");
					Trade::from(parsed)
				}
				_ => unimplemented!(),
			};
			if trade.price == 0.0 || trade.qty_asset == 0.0 {
				tracing::debug!(
					raw_json = %content_event.data,
					topic = %content_event.topic,
					event_type = %content_event.event_type,
					event_time = %content_event.time,
					"Binance sent a zero-valued trade, skipping.\nWas deserialized to: {trade:?}\nReportedly, means non-orderbook trades. Look at `X` value for more info (could be in: {{ADL, INSURANCE_FUND, NA}})",
				);
				continue;
			}
			return Ok(trade);
		}
	}
}

#[serde_as]
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct TradeEventPerp {
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
impl From<TradeEventPerp> for Trade {
	fn from(futs: TradeEventPerp) -> Self {
		Self {
			time: Timestamp::from_millisecond(futs.timestamp).expect("Exchange responded with invalid timestamp"),
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
impl From<TradeEventSpot> for Trade {
	fn from(futs: TradeEventSpot) -> Self {
		Self {
			time: Timestamp::from_millisecond(futs.timestamp).expect("Exchange responded with invalid timestamp"),
			qty_asset: futs.qty_asset,
			price: futs.price,
		}
	}
}

//,}}}
