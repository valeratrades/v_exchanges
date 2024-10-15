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
	BinanceError(BinanceContentError),

	#[error("Kline value '{name}' at index {index} is missing")]
	KlineValueMissingError { index: usize, name: &'static str },

	#[error(transparent)]
	ReqError(#[from] reqwest::Error),

	#[error(transparent)]
	InvalidHeaderError(#[from] reqwest::header::InvalidHeaderValue),

	#[error(transparent)]
	IoError(#[from] std::io::Error),

	#[error(transparent)]
	ParseFloatError(#[from] std::num::ParseFloatError),

	#[error(transparent)]
	UrlParserError(#[from] url::ParseError),

	#[error(transparent)]
	JsonError(#[from] serde_json::Error),

	#[error(transparent)]
	TungsteniteError(#[from] tungstenite::Error),

	#[error(transparent)]
	TimestampError(#[from] std::time::SystemTimeError),
}
