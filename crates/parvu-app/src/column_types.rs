//! Column types the user chose for a file ("Read as" in the column menu)
//! and how to parse it ("Open with options"), remembered in
//! `~/.ParVu/column_types.json` and applied when the same file opens again. An entry only applies while the file has the size and
//! modification time it had when the types were chosen.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use parvu_core::types::{CsvOptions, Reading};
use serde::{Deserialize, Serialize};

/// Files remembered at most; the oldest choices are dropped first.
const MAX_FILES: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Entry {
    size: u64,
    modified: u64,
    /// When the types were last chosen (seconds), to drop the oldest.
    saved: u64,
    columns: BTreeMap<String, Reading>,
    /// Parsing options from "Open with options".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    csv: Option<CsvOptions>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ColumnTypes {
    files: BTreeMap<String, Entry>,
}

/// `(size, modified seconds)` of a file.
fn stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some((meta.len(), modified))
}

fn key(path: &Path) -> String {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()).to_string_lossy().into_owned()
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl ColumnTypes {
    pub fn file() -> PathBuf {
        parvu_config::paths::legacy_app_dir().join("column_types.json")
    }

    /// The remembered choices, or none when the file is missing or unreadable.
    pub fn load(file: &Path) -> Self {
        std::fs::read_to_string(file).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self, file: &Path) -> std::io::Result<()> {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(file, serde_json::to_string_pretty(self)?)
    }

    /// Readings chosen for `path`, if the file is unchanged since.
    pub fn for_file(&self, path: &Path) -> Vec<(String, Reading)> {
        let Some(entry) = self.files.get(&key(path)) else { return Vec::new() };
        if stamp(path) != Some((entry.size, entry.modified)) {
            return Vec::new();
        }
        entry.columns.iter().map(|(c, r)| (c.clone(), r.clone())).collect()
    }

    /// Parsing options chosen for `path`, if the file is unchanged since.
    pub fn csv_for(&self, path: &Path) -> Option<CsvOptions> {
        let entry = self.files.get(&key(path))?;
        (stamp(path) == Some((entry.size, entry.modified))).then(|| entry.csv.clone()).flatten()
    }

    /// The entry of `path`, emptied when the file changed since.
    fn entry(&mut self, path: &Path) -> Option<&mut Entry> {
        let (size, modified) = stamp(path)?;
        let entry = self.files.entry(key(path)).or_insert_with(|| Entry {
            size,
            modified,
            saved: 0,
            columns: BTreeMap::new(),
            csv: None,
        });
        if (entry.size, entry.modified) != (size, modified) {
            entry.columns.clear();
            entry.csv = None;
            (entry.size, entry.modified) = (size, modified);
        }
        entry.saved = now();
        Some(entry)
    }

    /// Remember how to parse `path`.
    pub fn remember_csv(&mut self, path: &Path, csv: CsvOptions) {
        if let Some(entry) = self.entry(path) {
            entry.csv = Some(csv);
        }
        self.trim();
    }

    /// Remember that `column` of `path` is read as `reading`.
    pub fn remember(&mut self, path: &Path, column: &str, reading: Reading) {
        let Some(entry) = self.entry(path) else { return };
        entry.columns.insert(column.to_string(), reading);
        self.trim();
    }

    /// Keep at most [`MAX_FILES`] entries, dropping the oldest.
    fn trim(&mut self) {
        while self.files.len() > MAX_FILES {
            let oldest = self.files.iter().min_by_key(|(_, e)| e.saved).map(|(k, _)| k.clone());
            match oldest {
                Some(k) => self.files.remove(&k),
                None => break,
            };
        }
    }

    /// Forget a choice (the column is read as detected again).
    pub fn forget(&mut self, path: &Path, column: &str) {
        if let Some(entry) = self.files.get_mut(&key(path)) {
            entry.columns.remove(column);
            if entry.columns.is_empty() && entry.csv.is_none() {
                self.files.remove(&key(path));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_until_the_file_changes() {
        let dir = tempfile::TempDir::new().unwrap();
        let data = dir.path().join("a.csv");
        std::fs::write(&data, "x\n1\n").unwrap();
        let store_file = dir.path().join("column_types.json");

        let mut store = ColumnTypes::default();
        store.remember(&data, "x", Reading::Text);
        store.save(&store_file).unwrap();
        let store = ColumnTypes::load(&store_file);
        assert_eq!(store.for_file(&data), vec![("x".to_string(), Reading::Text)]);

        // Another size: the file changed, the choice no longer applies.
        std::fs::write(&data, "x\n1\n2\n").unwrap();
        assert!(store.for_file(&data).is_empty());
    }

    #[test]
    fn forgets_a_column() {
        let dir = tempfile::TempDir::new().unwrap();
        let data = dir.path().join("a.csv");
        std::fs::write(&data, "x\n1\n").unwrap();
        let mut store = ColumnTypes::default();
        store.remember(&data, "x", Reading::Integer);
        store.forget(&data, "x");
        assert!(store.for_file(&data).is_empty());
        assert!(store.files.is_empty());
    }

    #[test]
    fn remembers_parsing_options() {
        let dir = tempfile::TempDir::new().unwrap();
        let data = dir.path().join("a.csv");
        std::fs::write(&data, "x;y\n1;2\n").unwrap();
        let mut store = ColumnTypes::default();
        let csv = CsvOptions { delimiter: Some(";".into()), ..Default::default() };
        store.remember_csv(&data, csv.clone());
        store.remember(&data, "x", Reading::Text);
        assert_eq!(store.csv_for(&data), Some(csv));
        store.forget(&data, "x");
        assert!(store.csv_for(&data).is_some(), "forgetting a column keeps the options");
        std::fs::write(&data, "x;y\n1;2\n3;4\n").unwrap();
        assert_eq!(store.csv_for(&data), None);
    }
}
