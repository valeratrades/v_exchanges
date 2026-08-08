mod account;
mod history;
mod market;
mod ws;

use std::collections::BTreeMap;

use adapters::bybit::{BybitOption, BybitOptions};
use exchange_interactions_adapters::{Client, GetOptions};
use secrecy::SecretString;
use trading_data_core::Pair;
use v_utils::Timeframe;

use crate::{
	BatchTrades, BookUpdate, ExchangeError, ExchangeInfo, ExchangeName, ExchangeResult, ExchangeStream, Instrument, MethodError, OpenInterest, PrecisionPriceQty, Symbol,
	core::{Account, ExchangeSeal, History, Klines, Market, PersonalInfo, RequestRange, Stream, validate_recv_window},
};

#[derive(Clone, Debug, Default, derive_more::Deref, derive_more::DerefMut)]
pub struct Bybit {
	#[deref]
	#[deref_mut]
	pub client: Client,
	pub info_cache: BTreeMap<Instrument, ExchangeInfo>,
}
impl Bybit {
	/// Both ws streams decode frames against the venue's per-pair precision, and a second subscribe
	/// on the same instrument must not re-fetch `exchange_info`.
	async fn pair_precisions(&mut self, pairs: &[Pair], instrument: Instrument) -> ExchangeResult<BTreeMap<Pair, PrecisionPriceQty>> {
		if !self.info_cache.contains_key(&instrument) {
			let info = Market::exchange_info(&*self, instrument).await?;
			self.info_cache.insert(instrument, info);
		}
		let exchange = self.name();
		let info = self.info_cache.get(&instrument).expect("just inserted or was present");
		pairs
			.iter()
			.map(|pair| {
				info.pairs
					.get(pair)
					.ok_or_else(|| ExchangeError::Method(MethodError::new_pair_not_listed(exchange, instrument, *pair)))
					.map(|pi| {
						(
							*pair,
							PrecisionPriceQty {
								price: pi.price_precision,
								qty: pi.qty_precision,
							},
						)
					})
			})
			.collect()
	}
}

impl ExchangeSeal for Bybit {
	fn stream(&mut self) -> Option<&mut dyn Stream> {
		Some(self)
	}

	fn history(&self) -> Option<&dyn History> {
		Some(self)
	}
}

#[async_trait::async_trait]
impl History for Bybit {
	async fn trades(&self, symbol: Symbol, since: jiff::Timestamp, until: jiff::Timestamp) -> ExchangeResult<Box<dyn ExchangeStream<Item = BatchTrades>>> {
		let info = Market::exchange_info(self, symbol.instrument).await?;
		Ok(Box::new(history::ArchiveTrades::new(symbol, history::precision(&info, symbol)?, history::window(since, until)?)))
	}

	async fn book(&self, symbol: Symbol, since: jiff::Timestamp, until: jiff::Timestamp) -> ExchangeResult<Box<dyn ExchangeStream<Item = BookUpdate>>> {
		let info = Market::exchange_info(self, symbol.instrument).await?;
		Ok(Box::new(history::ArchiveBook::new(symbol, history::precision(&info, symbol)?, history::window(since, until)?)))
	}
}

#[async_trait::async_trait]
impl Account for Bybit {
	fn auth(&mut self, pubkey: String, secret: SecretString) {
		self.update_default_option(BybitOption::Pubkey(pubkey));
		self.update_default_option(BybitOption::Secret(secret));
	}

	fn set_recv_window(&mut self, recv_window: std::time::Duration) {
		self.update_default_option(BybitOption::RecvWindow(recv_window));
	}

	fn default_recv_window(&self) -> Option<std::time::Duration> {
		GetOptions::<BybitOptions>::default_options(&**self).recv_window
	}

	async fn personal_info(&self, _instrument: Instrument, recv_window: Option<std::time::Duration>) -> ExchangeResult<PersonalInfo> {
		validate_recv_window(recv_window, self.default_recv_window())?;
		account::personal_info(self, recv_window).await
	}
}

//? currently client ends up importing this from crate::binance, but could it be possible to lift the [Client] reexport up, and still have the ability to call all exchange methods right on it?
#[async_trait::async_trait]
impl Market for Bybit {
	fn name(&self) -> ExchangeName {
		ExchangeName::Bybit
	}

	async fn exchange_info(&self, instrument: Instrument) -> ExchangeResult<ExchangeInfo> {
		match instrument {
			Instrument::Perp | Instrument::PerpInverse | Instrument::Spot => market::exchange_info(self, instrument).await,
			_ => unimplemented!(),
		}
	}

	async fn klines(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Klines> {
		match symbol.instrument {
			Instrument::Perp => market::klines(self, symbol, tf.try_into()?, range).await,
			_ => unimplemented!(),
		}
	}

	async fn prices(&self, pairs: Option<Vec<Pair>>, instrument: Instrument) -> ExchangeResult<BTreeMap<Pair, f64>> {
		match instrument {
			Instrument::Perp => market::prices(self, pairs, instrument).await,
			_ => unimplemented!(),
		}
	}

	async fn open_interest(&self, symbol: Symbol, tf: Timeframe, range: RequestRange) -> ExchangeResult<Vec<OpenInterest>> {
		match symbol.instrument {
			Instrument::Perp => market::open_interest(self, symbol, tf.try_into()?, range).await,
			_ => Err(crate::ExchangeError::Method(crate::MethodError::new_method_not_supported(self.name(), symbol.instrument))),
		}
	}
}

#[async_trait::async_trait]
impl Stream for Bybit {
	async fn ws_book(&mut self, pairs: &[Pair], instrument: Instrument) -> Result<Box<dyn ExchangeStream<Item = BookUpdate>>, ExchangeError> {
		match instrument {
			Instrument::Perp | Instrument::Spot => {
				let pair_precisions = self.pair_precisions(pairs, instrument).await?;
				let connection = ws::BookConnection::try_new(self, pairs, instrument, pair_precisions)?;
				Ok(Box::new(connection))
			}
			_ => Err(ExchangeError::Method(MethodError::new_method_not_implemented(self.name(), instrument))),
		}
	}

	async fn ws_trades(&mut self, pairs: &[Pair], instrument: Instrument) -> Result<Box<dyn ExchangeStream<Item = BatchTrades>>, ExchangeError> {
		match instrument {
			Instrument::Perp | Instrument::Spot => {
				let pair_precisions = self.pair_precisions(pairs, instrument).await?;
				let connection = ws::TradeConnection::try_new(self, pairs, instrument, pair_precisions)?;
				Ok(Box::new(connection))
			}
			_ => Err(ExchangeError::Method(MethodError::new_method_not_implemented(self.name(), instrument))),
		}
	}
}

crate::define_provider_timeframe!(BybitInterval, ["1", "3", "5", "15", "30", "60", "120", "240", "360", "720", "D", "W", "M"]);
crate::define_provider_timeframe!(BybitIntervalTime, ["5min", "15min", "30min", "1h", "4h", "1d"]);
