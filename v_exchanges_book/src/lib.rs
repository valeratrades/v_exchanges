use std::{path::PathBuf, sync::Arc};

use url::Url;
use v_exchanges_methods::{self, Exchange, Ticker};

pub struct Book;

/* DO:
will be split into Hot and Cold parts.

Cold can run on a different port, and even on a different machine. Hot needs to subscribe to Cold. // obviously will allow to have it spawn up Cold in an owned process if needed, without going through subscriptions layer

Hot will have an always up-to-date snapshot state + a way to wait for its update (tick)

Cold will directly own per-day parquet files with data per supported exchange (different files for different exchanges).
Has a method to request entire history for any Timestamp:Timestamp. When requesting, you can filter in/out any pattern like exchanges or depths; but when it's returned, everything is flat and ordered by creation time (NB: not arrival). Trades go in a separate line (cause storage and processing optimizations).
*/

//DO: need a daemon, subscribable over TCP // cause what if multiple things want same data + storage problems

/*DO:
/// want to have distinction over what
enum Data {
	Snapshot,
	Delta,
	Trade,
	/// Session close / trading halted randomly / delisted
	Close,
}
*/

/// self-managed. One of the oh so few things allowed to have its proper tokio::spawn
//Q: wait, does it have to? If it writes to kernel buffer anyways, could we make it be pull-based as everything else is?
struct BookCold {}
impl BookCold {
	pub fn new(storage_dir: PathBuf, clients: &[Arc<Box<dyn Exchange>>], tickers: &[Ticker]) -> Self {
		todo!();
	}
}

enum ColdSource {
	Path(PathBuf),
	//Q: do we need to make a distinction between www communications and same pc tcp?
	Url(Url),
}

/// provides a channel to Cold. Its step is batch-reading from it
//struct BookHot {
//	pub snapshot: UnsafeCell<BookShape>,
//	cache: todo!();, // needs to be persisting hot loaded areas of the map. In reality, BookHot itself should be a thin wrapper around BookCold I think. Only difference is we warm all the data contained + is always local.
//}
//impl BookHot {
//	pub fn new(cold_source: ColdSource) -> Self {
//		todo!();
//	}
//}
