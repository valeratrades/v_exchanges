//! Bybit's public archives: the daily trade tape at `public.bybit.com`, and the ob200/ob500
//! orderbook dumps at `quote-saver.bycsi.com`.
//!
//! Raw bytes are staged under `/tmp` and never leave this module — see [`stage`]. What the streams
//! yield is already scaled to the venue's own tick.

use std::{
	collections::{BTreeMap, VecDeque},
	fs,
	io::{BufRead as _, BufReader, Lines, Write as _},
	path::{Path, PathBuf},
};

use adapters::generics::ws::WsError;
use jiff::{Timestamp, civil::Date, tz::TimeZone};
use trading_data_core::Span;

use crate::{ExchangeStream, prelude::*};

/// CloudFront 403s reqwest's default agent string on the quote-saver host; any custom one passes.
const UA: &str = "exchange_interactions";
const GZIP_MAGIC: &[u8] = &[0x1f, 0x8b];
const ZIP_MAGIC: &[u8] = b"PK\x03\x04";
/// Trades handed over per [`ExchangeStream::next`]. A day's tape runs to millions of rows; this is
/// what keeps the consumer's footprint the batch rather than the window.
const TRADE_BATCH: usize = 100_000;
/// Archive messages handed over per [`ExchangeStream::next`], same reason.
const BOOK_BATCH: usize = 4_096;
/// Bybit publishes one or the other per symbol-era, and the depth is in the file name. Probed in
/// this order; the first that answers is the one that exists.
const OB_DEPTHS: [u16; 2] = [200, 500];

fn category(instrument: Instrument) -> &'static str {
	match instrument {
		Instrument::Perp => "linear",
		Instrument::PerpInverse => "inverse",
		Instrument::Spot => "spot",
		_ => unimplemented!("Bybit publishes no archive for {instrument:?}"),
	}
}

/// The UTC days `[since, until)` touches. An `until` landing exactly on a midnight excludes that
/// day, which is what makes day-aligned windows round-trip.
fn days(since: Timestamp, until: Timestamp) -> Vec<Date> {
	let mut d = since.to_zoned(TimeZone::UTC).date();
	let mut out = Vec::new();
	while d.to_zoned(TimeZone::UTC).expect("a UTC date is always representable").timestamp() < until {
		out.push(d);
		d = d.tomorrow().expect("date is in range");
	}
	out
}

/// Where raw lives: `/tmp` is the OS's to reclaim, and the nesting exists only so two symbols or two
/// venues cannot collide. Reprocessing re-downloads — that is what raw being scratch costs.
fn scratch(instrument: Instrument, symbol: &str) -> PathBuf {
	let dir = PathBuf::from("/tmp/exchange_interactions/bybit").join(category(instrument)).join(symbol);
	fs::create_dir_all(&dir).expect("create archive scratch dir");
	dir
}

/// Downloads `url` to `to` unless it is already there, and answers `false` if the host has no such
/// archive. `magic` is the format's leading bytes: a size threshold would be a guess — a thin
/// instrument's day is legitimately tiny — whereas the wrong magic is exactly the truncation or
/// error page we must not cache. The rename only happens once the body is what it claims to be.
async fn stage(to: &Path, url: &str, magic: &[u8]) -> ExchangeResult<bool> {
	if to.exists() {
		return Ok(true);
	}
	let mut resp = reqwest::Client::new()
		.get(url)
		.header("user-agent", UA)
		.send()
		.await
		.map_err(|e| ExchangeError::Other(eyre!("GET {url}: {e}")))?;
	if resp.status() == reqwest::StatusCode::NOT_FOUND || resp.status() == reqwest::StatusCode::FORBIDDEN {
		return Ok(false);
	}
	if !resp.status().is_success() {
		return Err(ExchangeError::Other(eyre!("GET {url} returned {}", resp.status())));
	}

	let tmp = to.with_extension("part");
	let mut out = fs::File::create(&tmp).expect("create archive part");
	let mut head: Vec<u8> = Vec::with_capacity(magic.len());
	while let Some(chunk) = resp.chunk().await.map_err(|e| ExchangeError::Other(eyre!("GET {url}: {e}")))? {
		if head.len() < magic.len() {
			let take = (magic.len() - head.len()).min(chunk.len());
			head.extend_from_slice(&chunk[..take]);
		}
		out.write_all(&chunk).expect("write archive");
	}
	if head != magic {
		fs::remove_file(&tmp).expect("drop rejected archive part");
		return Err(ExchangeError::Other(eyre!("GET {url} is not the expected archive format")));
	}
	out.flush().expect("flush archive");
	fs::rename(&tmp, to).expect("move archive into place");
	Ok(true)
}

/// The venue's own tick, which is what the archives' decimal strings are stated in.
pub(super) fn precision(info: &ExchangeInfo, symbol: Symbol) -> ExchangeResult<PrecisionPriceQty> {
	let pi = info
		.pairs
		.get(&symbol.pair)
		.ok_or_else(|| ExchangeError::Method(MethodError::new_pair_not_listed(ExchangeName::Bybit, symbol.instrument, symbol.pair)))?;
	Ok(PrecisionPriceQty {
		price: pi.price_precision,
		qty: pi.qty_precision,
	})
}

pub(super) fn window(since: Timestamp, until: Timestamp) -> ExchangeResult<VecDeque<Date>> {
	match days(since, until) {
		d if d.is_empty() => Err(ExchangeError::Other(eyre!("archive window [{since}, {until}) covers no UTC day"))),
		d => Ok(d.into()),
	}
}

// trades {{{
/// Bybit's csv.gz trade tape over a window, decoded [`TRADE_BATCH`] rows at a time.
#[derive(Debug)]
pub(super) struct ArchiveTrades {
	prec: PrecisionPriceQty,
	symbol: Symbol,
	pending: VecDeque<Date>,
	lines: Option<Lines<BufReader<flate2::read::GzDecoder<fs::File>>>>,
	/// The tape is time-ordered and everything downstream relies on that; check it once, here.
	prev: i64,
}

impl ArchiveTrades {
	pub(super) fn new(symbol: Symbol, prec: PrecisionPriceQty, window: VecDeque<Date>) -> Self {
		Self {
			prec,
			symbol,
			pending: window,
			lines: None,
			prev: i64::MIN,
		}
	}

	/// Opens the next day's tape, or answers `false` once the window is spent.
	async fn advance(&mut self) -> ExchangeResult<bool> {
		let Some(day) = self.pending.pop_front() else { return Ok(false) };
		let sym = self.symbol.pair.fmt_bybit();
		let name = format!("{sym}{day}.csv.gz");
		let to = scratch(self.symbol.instrument, &sym).join(&name);
		let url = format!("https://public.bybit.com/trading/{sym}/{name}");
		if !stage(&to, &url, GZIP_MAGIC).await? {
			return Err(ExchangeError::Other(eyre!("Bybit publishes no trade archive at {url}")));
		}

		let file = fs::File::open(&to).expect("open staged archive");
		let mut lines = BufReader::new(flate2::read::GzDecoder::new(file)).lines();
		let header = lines.next().expect("archive is empty").expect("read header");
		assert!(header.starts_with("timestamp,symbol,side,size,price"), "unexpected header in {name}: {header}");
		self.lines = Some(lines);
		Ok(true)
	}
}

#[async_trait::async_trait]
impl ExchangeStream for ArchiveTrades {
	type Item = BatchTrades;

	/// `Ok(vec![])` is the window running out — a live socket never yields an empty batch, so it is
	/// free to mean exhausted here.
	async fn next(&mut self) -> Result<Vec<Self::Item>, WsError> {
		let sym = self.symbol.pair.fmt_bybit();
		loop {
			let Some(lines) = &mut self.lines else {
				if !self.advance().await.map_err(|e| WsError::Other(eyre!("{e}")))? {
					return Ok(Vec::new());
				}
				continue;
			};
			let raw: Vec<String> = lines.by_ref().take(TRADE_BATCH).map(|l| l.expect("read archive line")).collect();
			if raw.is_empty() {
				self.lines = None;
				continue;
			}

			let mut trades = Vec::with_capacity(raw.len());
			for line in &raw {
				let mut cols = line.split(',');
				let mut col = || cols.next().unwrap_or_else(|| panic!("malformed archive line: {line}"));
				let ts_sec: f64 = col().parse().unwrap_or_else(|e| panic!("bad timestamp in `{line}`: {e}"));
				assert_eq!(col(), sym, "foreign symbol in `{line}`");
				let side: Side = col().parse().unwrap_or_else(|e| panic!("bad side in `{line}`: {e}"));
				let qty = self.prec.qty.parse_u32(col());
				let price = self.prec.price.parse_i32(col());

				let ts = (ts_sec * 1e9).round() as i64;
				assert!(ts >= self.prev, "archive is not time-ordered: {} > {ts}", self.prev);
				self.prev = ts;
				trades.push(InnerTrade {
					time: Ts::from_nanos(ts),
					sent: None,
					price,
					qty,
					side,
				});
			}
			// An archive was never received, so reception collapses onto the event. Anything that
			// records a reading of its own overwrites this on ingest; a historic ingest keeps none.
			let recv = Ts::<Local>::from_nanos(trades.last().expect("raw was non-empty").time.as_nanos());
			return Ok(vec![BatchTrades::new(self.prec, trades, recv)]);
		}
	}
}
//,}}}

// book {{{
/// The ob200/ob500 archives over a window, decoded [`BOOK_BATCH`] messages at a time. The stream is
/// the venue's own, carried across files, so a day boundary is just another message.
#[derive(Debug)]
pub(super) struct ArchiveBook {
	prec: PrecisionPriceQty,
	symbol: Symbol,
	pending: VecDeque<Date>,
	/// Held as the reader rather than its [`Lines`]: a 400-level message is ~10KB, and `lines()`
	/// allocates and drops that much per message where [`Self::line`] is reused across the file.
	reader: Option<BufReader<fs::File>>,
	line: String,
	prev: i64,
}

impl ArchiveBook {
	pub(super) fn new(symbol: Symbol, prec: PrecisionPriceQty, window: VecDeque<Date>) -> Self {
		Self {
			prec,
			symbol,
			pending: window,
			reader: None,
			line: String::new(),
			prev: i64::MIN,
		}
	}

	async fn advance(&mut self) -> ExchangeResult<bool> {
		let Some(day) = self.pending.pop_front() else { return Ok(false) };
		let sym = self.symbol.pair.fmt_bybit();
		let cat = category(self.symbol.instrument);
		let dir = scratch(self.symbol.instrument, &sym);

		let mut zipped = None;
		for depth in OB_DEPTHS {
			let name = format!("{day}_{sym}_ob{depth}.data.zip");
			let to = dir.join(&name);
			if stage(&to, &format!("https://quote-saver.bycsi.com/orderbook/{cat}/{sym}/{name}"), ZIP_MAGIC).await? {
				zipped = Some(to);
				break;
			}
		}
		let zipped = zipped.ok_or_else(|| ExchangeError::Other(eyre!("Bybit publishes no ob{OB_DEPTHS:?} archive for {sym} on {day}")))?;

		// Unpacked beside the zip rather than streamed out of it: `ZipFile` borrows its archive, and
		// a member that outlives the call is worth more here than the disk a scratch copy costs.
		let jsonl = zipped.with_extension("jsonl");
		if !jsonl.exists() {
			let mut archive = zip::ZipArchive::new(fs::File::open(&zipped).expect("open staged archive")).expect("book archive is a zip");
			assert_eq!(archive.len(), 1, "an orderbook archive holds exactly one jsonl member");
			let tmp = zipped.with_extension("part");
			let mut out = fs::File::create(&tmp).expect("create unpacked part");
			std::io::copy(&mut archive.by_index(0).expect("open zip member"), &mut out).expect("unpack archive");
			fs::rename(&tmp, &jsonl).expect("move unpacked archive into place");
		}
		self.reader = Some(BufReader::new(fs::File::open(&jsonl).expect("open unpacked archive")));
		Ok(true)
	}
}

/// One archive record, borrowed out of the line buffer. A [`serde_json::Value`] of the same line is
/// a `String` per key and per number — ~1600 allocations to reach the 800 integers a 400-level
/// message carries — where the derive resolves the keys at compile time and leaves the numbers as
/// slices for [`Precision`] to walk. Fields nothing downstream reads (`topic`, `data.s`, the update
/// ids) are simply not named.
#[derive(serde::Deserialize)]
struct Rec<'a> {
	ts: i64,
	#[serde(rename = "type")]
	kind: &'a str,
	#[serde(borrow)]
	data: Sides<'a>,
}

#[derive(serde::Deserialize)]
struct Sides<'a> {
	#[serde(borrow)]
	b: Vec<[&'a str; 2]>,
	#[serde(borrow)]
	a: Vec<[&'a str; 2]>,
}

/// Free rather than a method so the batch loop can hold the line buffer and the cursor at once.
fn parse(line: &str, prec: PrecisionPriceQty, prev: &mut i64) -> BookUpdate {
	let rec: Rec = serde_json::from_str(line).unwrap_or_else(|e| panic!("malformed archive line: {e}"));
	let ts_ns = rec.ts * 1_000_000;
	assert!(ts_ns >= *prev, "archive is not time-ordered: {prev} > {ts_ns}");
	*prev = ts_ns;

	let levels = |side: &[[&str; 2]]| -> BTreeMap<i32, u32> { side.iter().map(|[p, q]| (prec.price.parse_i32(p), prec.qty.parse_u32(q))).collect() };
	let shape = BookShape {
		ts: Aggregate {
			venue_exec: Span::at(Ts::<Venue>::from_nanos(ts_ns)),
			// As with a trade archive's: nothing here was ever received.
			local_recv: Span::at(Ts::<Local>::from_nanos(ts_ns)),
		},
		prec,
		bids: levels(&rec.data.b),
		asks: levels(&rec.data.a),
	};
	match rec.kind {
		"snapshot" => BookUpdate::Snapshot(shape),
		// The archive is the venue's own recollection, so there is no sequence to have broken.
		"delta" => BookUpdate::BatchDelta { shape, gapped: false },
		other => panic!("unknown archive record type `{other}`"),
	}
}

#[async_trait::async_trait]
impl ExchangeStream for ArchiveBook {
	type Item = BookUpdate;

	/// `Ok(vec![])` is the window running out; see [`ArchiveTrades::next`].
	async fn next(&mut self) -> Result<Vec<Self::Item>, WsError> {
		loop {
			let Some(reader) = &mut self.reader else {
				if !self.advance().await.map_err(|e| WsError::Other(eyre!("{e}")))? {
					return Ok(Vec::new());
				}
				continue;
			};
			let mut batch = Vec::with_capacity(BOOK_BATCH);
			while batch.len() < BOOK_BATCH {
				self.line.clear();
				if reader.read_line(&mut self.line).expect("read archive line") == 0 {
					break;
				}
				batch.push(parse(&self.line, self.prec, &mut self.prev));
			}
			if batch.is_empty() {
				self.reader = None;
				continue;
			}
			return Ok(batch);
		}
	}
}
//,}}}
