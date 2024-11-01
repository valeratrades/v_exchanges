use color_eyre::eyre::Result;
use tokio::sync::mpsc;

//mod binance;

pub struct Pair {
	pub base: String,
	pub quote: String,
}

pub struct Kline {
	pub open_time: i64,
	pub ohlc: Ohlc,
	/// None if the candle is not closed yet
	pub close_time: Option<i64>,
	pub base_asset_volume: f64,
	pub quote_asset_volume: f64,
	pub number_of_trades: usize,
	pub taker_buy_base_asset_volume: f64,
	pub taker_buy_quote_asset_volume: f64,
}

pub struct Ohlc {
	pub open: f64,
	pub high: f64,
	pub low: f64,
	pub close: f64,
}

//pub struct Config {
//	binance: binance::BinanceConfig,
//}

pub trait Exchange {
	//? should I have Self::Pair too? Like to catch the non-existent ones immediately? Although this woudl increase the error surface on new listings.
	type Timeframe;
	fn klines(&self, symbol: Pair, tf: Self::Timeframe, limit: u32) -> impl std::future::Future<Output = Result<Kline>> + Send;
	// Defined in terms of actors
	fn spawn_klines_listener(&self, symbol: Pair, tf: Self::Timeframe) -> mpsc::Receiver<Kline>;

	//? Do I need to define functions like this? They seem to follow from the previous ones.They just follow naturally from previous reqs, don't they?
	//fn translate_timeframe(tf: Timeframe) -> Self::Timeframe;
	//fn format_pair(pair: Pair) -> String;
}
