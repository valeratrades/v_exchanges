use std::{path::PathBuf, sync::Arc};

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

struct BookCold {}
impl BookCold {
	pub fn new(storage_dir: PathBuf, clients: &[Arc<Box<dyn Exchange>>], tickers: &[Ticker]) -> Self {
		todo!();
	}
}

struct BookHot {}
