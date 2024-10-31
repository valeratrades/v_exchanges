use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Deserialize)]
pub struct BinanceContentError {
	pub code: i16,
	pub msg: String,
}

#[derive(Error, Debug)]
pub enum BinanceError {
	#[error("Binance API error: {0:?}")]
	Content(BinanceContentError),

	#[error("Kline value '{name}' at index {index} is missing")]
	KlineValueMissing { index: usize, name: &'static str },

	#[error(transparent)]
	Req(#[from] reqwest::Error),

	#[error(transparent)]
	InvalidHeader(#[from] reqwest::header::InvalidHeaderValue),

	#[error(transparent)]
	Io(#[from] std::io::Error),

	#[error(transparent)]
	ParseFloat(#[from] std::num::ParseFloatError),

	#[error(transparent)]
	UrlParser(#[from] url::ParseError),

	#[error(transparent)]
	Json(#[from] serde_json::Error),

	#[error(transparent)]
	Tungstenite(#[from] tungstenite::Error),

	#[error(transparent)]
	Timestamp(#[from] std::time::SystemTimeError),
}
