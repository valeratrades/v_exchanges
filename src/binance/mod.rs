use color_eyre::eyre::{bail, Result};
use futures_util::StreamExt as _;
use serde::Deserialize;
use serde_with::{serde_as, DisplayFromStr};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tracing::debug;
use v_utils::trades::Timeframe;

use crate::{Exchange, Kline, Ohlc, Pair};

pub mod errors;
mod model;

#[derive(Clone, Debug, Default)]
struct Binance {
	api_key: String,
	api_secret: String,
}
impl Binance {
	fn new(api_key: String, api_secret: String) -> Self {
		Self { api_key, api_secret }
	}

	fn format_pair(pair: Pair) -> String {
		format!("{}{}", pair.base, pair.quote)
	}

	fn translate_timeframe(tf: v_utils::trades::Timeframe) -> BinanceTimeframe {
		todo!()
	}
}

#[derive(Debug, Clone, PartialEq, Copy, Default)]
struct BinanceTimeframe(Timeframe);
impl TryFrom<Timeframe> for BinanceTimeframe {
	type Error = color_eyre::eyre::Error;

	#[inline(always)]
	fn try_from(tf: Timeframe) -> Result<Self, Self::Error> {
		let valid_values = vec![
			"1s", "5s", "15s", "30s", "1m", "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h", "8h", "12h", "1d", "3d", "1w", "1M",
		];
		//TODO: given n, go through the possible upgrades

		if !valid_values.contains(&tf.display().as_str()) {
			bail!("Unsupported timeframe");
		}

		Ok(Self(tf))
	}
}

impl Exchange for Binance {
	type Timeframe = BinanceTimeframe;

	async fn klines(&self, symbol: crate::Pair, tf: Self::Timeframe, limit: u32) -> color_eyre::eyre::Result<crate::Kline> { todo!() }

	//? wait, could I just get the klines once here, then only listen for AggrTrade updates, construct the candles on-site?
	// Also there should be a way to subscribe to multiple things at once, so this thing could check for existing subscriptions and then request being attached there.
	fn spawn_klines_listener(&self, symbol: Pair, tf: Self::Timeframe) -> mpsc::Receiver<Kline> {
		let (tx, rx) = mpsc::channel::<Kline>(256);
		let symbol = Self::format_pair(symbol);
		//CANCELLATION: will drop on first send to dropped rx
		tokio::spawn(async move {
			let address = format!("wss://fstream.binance.com/ws/{}@kline_{}", symbol.to_string().to_lowercase(), tf.0);
			let (ws_stream, _) = connect_async(address).await.unwrap();
			let (_, mut read) = ws_stream.split();

			while let Some(msg) = read.next().await {
				let data = msg.unwrap().into_data();
				debug!("SAR received websocket klines update: {:?}", data);
				match serde_json::from_slice::<WsKlineEvent>(&data) {
					Ok(kline_event) => match tx.send(kline_event.kline.into_kline()).await {
						Ok(()) => {}
						Err(_) => {
							break;
						}
					},
					Err(e) => {
						println!("Failed to parse message as JSON: {}", e);
					}
				}
			}
		});
		rx
	}
}

#[allow(dead_code)]
#[serde_as]
#[derive(Debug, Deserialize)]
struct WsKlineEvent {
	#[serde(rename = "e")]
	event_type: String,
	#[serde(rename = "E")]
	event_time: u64,
	#[serde(rename = "s")]
	symbol: String,
	#[serde(rename = "k")]
	kline: WsKlineData,
}

#[allow(dead_code)]
#[serde_as]
#[derive(Debug, Deserialize)]
struct WsKlineData {
	#[serde(rename = "t")]
	start_time: u64,
	#[serde(rename = "T")]
	close_time: u64,
	#[serde(rename = "s")]
	symbol: String,
	#[serde(rename = "i")]
	interval: String,
	#[serde(rename = "f")]
	first_trade_id: u64,
	#[serde(rename = "L")]
	last_trade_id: u64,
	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "o")]
	open: f64,
	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "c")]
	close: f64,
	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "h")]
	high: f64,
	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "l")]
	low: f64,
	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "v")]
	base_asset_volume: f64,
	#[serde(rename = "n")]
	number_of_trades: u64,
	#[serde(rename = "x")]
	is_closed: bool,
	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "q")]
	quote_asset_volume: f64,
	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "V")]
	taker_buy_base_asset_volume: f64,
	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "Q")]
	taker_buy_quote_asset_volume: f64,
	#[serde_as(as = "DisplayFromStr")]
	#[serde(rename = "B")]
	ignore: u64,
}
impl WsKlineData {
	pub fn into_kline(self) -> Kline {
		let ohlc = Ohlc {
			open: self.open,
			high: self.high,
			low: self.low,
			close: self.close,
		};

		Kline {
			open_time: self.start_time as i64,
			ohlc,
			close_time: if self.is_closed { Some(self.close_time as i64) } else { None },
			base_asset_volume: self.base_asset_volume,
			quote_asset_volume: self.quote_asset_volume,
			number_of_trades: self.number_of_trades as usize,
			taker_buy_base_asset_volume: self.taker_buy_base_asset_volume,
			taker_buy_quote_asset_volume: self.taker_buy_quote_asset_volume,
		}
	}
}

#[derive(Clone, Debug)]
pub struct BinanceConfig {
	pub rest_api_endpoint: String,
	pub ws_endpoint: String,

	pub futures_rest_api_endpoint: String,
	pub futures_ws_endpoint: String,

	pub recv_window: u64,
}

impl Default for BinanceConfig {
	fn default() -> Self {
		Self {
			rest_api_endpoint: "https://api.binance.com".into(),
			ws_endpoint: "wss://stream.binance.com/ws".into(),

			futures_rest_api_endpoint: "https://fapi.binance.com".into(),
			futures_ws_endpoint: "wss://fstream.binance.com/ws".into(),

			recv_window: 5000,
		}
	}
}

impl BinanceConfig {
	pub fn testnet() -> Self {
		Self::default()
			.set_rest_api_endpoint("https://testnet.binance.vision")
			.set_ws_endpoint("wss://testnet.binance.vision/ws")
			.set_futures_rest_api_endpoint("https://testnet.binancefuture.com")
			.set_futures_ws_endpoint("https://testnet.binancefuture.com/ws")
	}

	pub fn set_rest_api_endpoint<T: Into<String>>(mut self, rest_api_endpoint: T) -> Self {
		self.rest_api_endpoint = rest_api_endpoint.into();
		self
	}

	pub fn set_ws_endpoint<T: Into<String>>(mut self, ws_endpoint: T) -> Self {
		self.ws_endpoint = ws_endpoint.into();
		self
	}

	pub fn set_futures_rest_api_endpoint<T: Into<String>>(mut self, futures_rest_api_endpoint: T) -> Self {
		self.futures_rest_api_endpoint = futures_rest_api_endpoint.into();
		self
	}

	pub fn set_futures_ws_endpoint<T: Into<String>>(mut self, futures_ws_endpoint: T) -> Self {
		self.futures_ws_endpoint = futures_ws_endpoint.into();
		self
	}

	pub fn set_recv_window(mut self, recv_window: u64) -> Self {
		self.recv_window = recv_window;
		self
	}
}
