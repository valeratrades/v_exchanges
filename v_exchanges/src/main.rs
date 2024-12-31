use v_exchanges_adapters::binance::{BinanceHttpUrl, BinanceOption};
use v_exchanges_deser::{binance::Client, core::Exchange};

#[tokio::main]
async fn main() {
	color_eyre::install().unwrap();
	v_utils::utils::init_subscriber(v_utils::utils::LogDestination::xdg("v_exchanges"));

	let mut client = Client::new();
	client.update_default_option(BinanceOption::HttpUrl(BinanceHttpUrl::FuturesUsdM));

	let klines = client.klines(("BTC", "USDT").into(), "1m".into(), None, None, None).await.unwrap();
	dbg!(&klines);
}
