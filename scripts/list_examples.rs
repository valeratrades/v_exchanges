#!/usr/bin/env -S cargo -Zscript -q
---cargo
[package]
edition = "2024"
---

use std::{
	collections::BTreeMap,
	path::{Path, PathBuf},
};

fn main() {
	let mut by_dir: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
	collect(Path::new("."), false, &mut by_dir);

	let mut dirs: Vec<PathBuf> = by_dir.keys().cloned().collect();
	dirs.sort_by(|a, b| a.components().count().cmp(&b.components().count()).then(a.cmp(b)));

	for dir in &dirs {
		let mut files = by_dir[dir].clone();
		files.sort();
		for file in files {
			let name = file.file_stem().unwrap().to_str().unwrap();
			println!("- \"{name}\": {}", file.display());
		}
	}
}

fn collect(dir: &Path, in_examples: bool, by_dir: &mut BTreeMap<PathBuf, Vec<PathBuf>>) {
	let Ok(entries) = std::fs::read_dir(dir) else { return };
	let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
	if dir_name == "target" {
		return;
	}
	let in_examples = in_examples || dir_name == "examples";
	let mut subdirs = Vec::new();
	for entry in entries.flatten() {
		let path = entry.path();
		if path.is_dir() {
			subdirs.push(path);
		} else if in_examples && path.extension().is_some_and(|e| e == "rs") {
			by_dir.entry(dir.to_path_buf()).or_default().push(path);
		}
	}
	for subdir in subdirs {
		collect(&subdir, in_examples, by_dir);
	}
}
