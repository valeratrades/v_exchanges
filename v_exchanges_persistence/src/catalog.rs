//! Cold Parquet catalog. Filename `{ts_min}_{ts_max}.parquet` is the index — range pruning is
//! filename-only, no manifest. Per-lane subdirectories.
//!
//! ```text
//! {root}/data/snapshots/{exchange}_{pair}/{ts_min}_{ts_max}.parquet
//! {root}/data/deltas/{exchange}_{pair}/{ts_min}_{ts_max}.parquet
//! {root}/data/trades/{exchange}_{pair}/{ts_min}_{ts_max}.parquet
//! {root}/data/closes/{exchange}_{pair}/{ts_min}_{ts_max}.parquet
//! {root}/data/custom/{type_name}/{ts_min}_{ts_max}.parquet
//! ```

use std::{
	fs,
	path::{Path, PathBuf},
};

use arrow::array::RecordBatch;
use parquet::{
	arrow::{ArrowWriter, arrow_reader::ParquetRecordBatchReaderBuilder},
	basic::Compression,
	file::properties::WriterProperties,
};
use thiserror::Error;

use crate::schema::UnixNanos;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Lane {
	Snapshots,
	Deltas,
	Trades,
	Closes,
	Custom,
}

impl Lane {
	pub fn dir_name(self) -> &'static str {
		match self {
			Lane::Snapshots => "snapshots",
			Lane::Deltas => "deltas",
			Lane::Trades => "trades",
			Lane::Closes => "closes",
			Lane::Custom => "custom",
		}
	}

	/// Each lane chooses its compression: zstd for the heavy book-data lanes, snappy elsewhere.
	pub fn compression(self) -> Compression {
		match self {
			Lane::Snapshots | Lane::Deltas | Lane::Trades => Compression::ZSTD(parquet::basic::ZstdLevel::default()),
			Lane::Closes | Lane::Custom => Compression::SNAPPY,
		}
	}
}

#[derive(Clone, Debug)]
pub struct Catalog {
	root: PathBuf,
}
impl Catalog {
	pub fn new(root: impl Into<PathBuf>) -> Self {
		Self { root: root.into() }
	}

	pub fn root(&self) -> &Path {
		&self.root
	}

	pub fn lane_dir(&self, key: &LaneKey) -> PathBuf {
		self.root.join("data").join(key.lane.dir_name()).join(&key.pair_or_type)
	}

	/// Write a single batch to a new parquet file under `{lane_dir}/{ts_min}_{ts_max}.parquet`.
	/// Refuses to write if the new interval overlaps an existing file.
	pub fn write(&self, key: &LaneKey, batch: &RecordBatch, ts_min: UnixNanos, ts_max: UnixNanos) -> Result<PathBuf, CatalogError> {
		assert!(ts_min <= ts_max, "ts_min must be <= ts_max");

		let dir = self.lane_dir(key);
		fs::create_dir_all(&dir)?;

		let existing = self.list(key)?;
		for e in &existing {
			if intervals_overlap((e.ts_min, e.ts_max), (ts_min, ts_max)) {
				return Err(CatalogError::OverlappingInterval {
					existing: (e.ts_min, e.ts_max),
					new: (ts_min, ts_max),
				});
			}
		}

		let path = dir.join(format!("{ts_min}_{ts_max}.parquet"));
		let file = fs::File::create(&path)?;
		let props = WriterProperties::builder().set_compression(key.lane.compression()).build();
		let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(props))?;
		writer.write(batch)?;
		writer.close()?;
		Ok(path)
	}

	/// Lists every parquet file in the lane directory, sorted by `ts_min` ascending.
	pub fn list(&self, key: &LaneKey) -> Result<Vec<FileEntry>, CatalogError> {
		let dir = self.lane_dir(key);
		if !dir.exists() {
			return Ok(Vec::new());
		}
		let mut entries = Vec::new();
		for ent in fs::read_dir(&dir)? {
			let ent = ent?;
			let path = ent.path();
			if path.extension().and_then(|s| s.to_str()) != Some("parquet") {
				continue;
			}
			let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| CatalogError::BadFilename(path.display().to_string()))?;
			let (lo, hi) = stem.split_once('_').ok_or_else(|| CatalogError::BadFilename(path.display().to_string()))?;
			let ts_min: i64 = lo.parse().map_err(|_| CatalogError::BadFilename(path.display().to_string()))?;
			let ts_max: i64 = hi.parse().map_err(|_| CatalogError::BadFilename(path.display().to_string()))?;
			entries.push(FileEntry { path, ts_min, ts_max });
		}
		entries.sort_by_key(|e| e.ts_min);
		Ok(entries)
	}

	/// Returns files that may contain rows in `[start, end]` (inclusive). Filename-only pruning.
	pub fn list_range(&self, key: &LaneKey, start: UnixNanos, end: UnixNanos) -> Result<Vec<FileEntry>, CatalogError> {
		Ok(self.list(key)?.into_iter().filter(|e| e.ts_max >= start && e.ts_min <= end).collect())
	}

	/// Reads a parquet file into a vec of record batches. The reader applies row-group filtering on
	/// `ts_init` is left to the caller — replay paths just iterate everything in order.
	pub fn read(&self, path: &Path) -> Result<Vec<RecordBatch>, CatalogError> {
		let file = fs::File::open(path)?;
		let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
		let reader = builder.build()?;
		let mut out = Vec::new();
		for batch in reader {
			out.push(batch?);
		}
		Ok(out)
	}
}

#[derive(Debug, Error)]
pub enum CatalogError {
	#[error("io: {0}")]
	Io(#[from] std::io::Error),
	#[error("arrow: {0}")]
	Arrow(#[from] arrow::error::ArrowError),
	#[error("parquet: {0}")]
	Parquet(#[from] parquet::errors::ParquetError),
	#[error("write would create overlapping interval: existing {existing:?}, new {new:?}")]
	OverlappingInterval { existing: (UnixNanos, UnixNanos), new: (UnixNanos, UnixNanos) },
	#[error("malformed filename: {0}")]
	BadFilename(String),
}

/// Identifies a per-pair lane directory.
///
/// For `Custom`, `pair_or_type` is the type-name string the caller chose;
/// for the four book lanes, it's `{exchange}_{pair}`.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LaneKey {
	pub lane: Lane,
	/// e.g. `binance_BTC-USDT` for book lanes, or the type name for custom.
	pub pair_or_type: String,
}

impl LaneKey {
	pub fn book(lane: Lane, exchange: &str, pair: &str) -> Self {
		debug_assert!(!matches!(lane, Lane::Custom), "Custom lane uses type-name not exchange/pair");
		Self {
			lane,
			pair_or_type: format!("{exchange}_{pair}"),
		}
	}

	pub fn custom(type_name: &str) -> Self {
		Self {
			lane: Lane::Custom,
			pair_or_type: type_name.to_owned(),
		}
	}
}

/// Each entry corresponds to one immutable Parquet file in the catalog.
#[derive(Clone, Debug)]
pub struct FileEntry {
	pub path: PathBuf,
	pub ts_min: UnixNanos,
	pub ts_max: UnixNanos,
}

fn intervals_overlap(a: (UnixNanos, UnixNanos), b: (UnixNanos, UnixNanos)) -> bool {
	a.0 <= b.1 && b.0 <= a.1
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use arrow::array::{Int32Array, Int64Array, RecordBatch, UInt8Array, UInt32Array, UInt64Array};
	use tempfile::tempdir;

	use super::*;
	use crate::schema::{FileMetadata, lane_schema, with_metadata};

	fn meta() -> FileMetadata {
		FileMetadata {
			exchange: "binance".into(),
			pair: "BTC-USDT".into(),
			price_precision: 2,
			qty_precision: 5,
		}
	}

	/// One-row delta batch built directly via Arrow arrays. Catalog tests are about file I/O —
	/// row contents don't matter, only schema validity and `ts_min`/`ts_max` interval handling.
	fn one_delta_batch() -> RecordBatch {
		let schema = with_metadata(lane_schema(Lane::Deltas), meta());
		RecordBatch::try_new(
			schema,
			vec![
				Arc::new(Int64Array::from(vec![1_i64])),
				Arc::new(Int64Array::from(vec![1_i64])),
				Arc::new(UInt64Array::from(vec![1_u64])),
				Arc::new(UInt64Array::from(vec![1_u64])),
				Arc::new(UInt8Array::from(vec![0_u8])),
				Arc::new(Int32Array::from(vec![1_i32])),
				Arc::new(UInt32Array::from(vec![1_u32])),
			],
		)
		.unwrap()
	}

	#[test]
	fn write_list_read_round_trip() {
		let dir = tempdir().unwrap();
		let cat = Catalog::new(dir.path());
		let key = LaneKey::book(Lane::Deltas, "binance", "BTC-USDT");

		let batch = one_delta_batch();
		let path = cat.write(&key, &batch, 110, 210).unwrap();
		assert!(path.exists());

		let listed = cat.list(&key).unwrap();
		assert_eq!(listed.len(), 1);
		assert_eq!(listed[0].ts_min, 110);
		assert_eq!(listed[0].ts_max, 210);

		let read = cat.read(&listed[0].path).unwrap();
		assert_eq!(read.len(), 1);
		assert_eq!(read[0].num_rows(), 1);
	}

	#[test]
	fn refuses_overlapping_write() {
		let dir = tempdir().unwrap();
		let cat = Catalog::new(dir.path());
		let key = LaneKey::book(Lane::Deltas, "binance", "BTC-USDT");
		let batch = one_delta_batch();
		cat.write(&key, &batch, 100, 200).unwrap();
		let err = cat.write(&key, &batch, 150, 250).unwrap_err();
		assert!(matches!(err, CatalogError::OverlappingInterval { .. }));
	}

	#[test]
	fn list_range_prunes() {
		let dir = tempdir().unwrap();
		let cat = Catalog::new(dir.path());
		let key = LaneKey::book(Lane::Deltas, "binance", "BTC-USDT");
		let batch = one_delta_batch();
		cat.write(&key, &batch, 100, 200).unwrap();
		cat.write(&key, &batch, 300, 400).unwrap();
		cat.write(&key, &batch, 500, 600).unwrap();

		let pruned = cat.list_range(&key, 250, 450).unwrap();
		assert_eq!(pruned.len(), 1);
		assert_eq!(pruned[0].ts_min, 300);
	}
}
