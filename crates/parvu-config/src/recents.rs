//! Recently opened files, ported from `config/recents.py`.
//!
//! File format: `{"recents": ["/path/a", "/path/b"]}`, newest first. Adding
//! a path moves it to the front and removes any earlier copy. Like the Python
//! app there is no length limit.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, write_atomic};
use crate::paths::AppPaths;
use crate::settings::Settings;

/// Contents of `recents.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentsData {
    #[serde(default)]
    pub recents: Vec<String>,
}

/// Manages `history/recents.json`.
#[derive(Debug, Clone)]
pub struct RecentsManager {
    file: PathBuf,
    data: RecentsData,
}

impl Default for RecentsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RecentsManager {
    /// Use `~/.ParVu/history/recents.json`.
    pub fn new() -> Self {
        Self::with_paths(&AppPaths::default())
    }

    /// Use `<base>/history/recents.json`.
    pub fn with_base_dir(base: impl AsRef<Path>) -> Self {
        Self::with_paths(&AppPaths::new(base.as_ref()))
    }

    pub fn with_paths(paths: &AppPaths) -> Self {
        Self::with_file(paths.recents_file())
    }

    /// Use an explicit file. A missing or corrupt file gives an empty list.
    pub fn with_file(file: impl Into<PathBuf>) -> Self {
        let file = file.into();
        let data = std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| match serde_json::from_str(&t) {
                Ok(d) => Some(d),
                Err(e) => {
                    tracing::warn!("Ignoring unreadable recents file {}: {e}", file.display());
                    None
                }
            })
            .unwrap_or_default();
        Self { file, data }
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    /// Recent file paths, newest first.
    pub fn recents(&self) -> &[String] {
        &self.data.recents
    }

    /// Write the list to disk. Errors are logged and returned.
    pub fn save(&self) -> Result<()> {
        let result = serde_json::to_string_pretty(&self.data)
            .map_err(Into::into)
            .and_then(|json| write_atomic(&self.file, &json));
        if let Err(e) = &result {
            tracing::error!("Failed to save recents: {e}");
        }
        result
    }

    /// Move `path` to the front (removing duplicates) and save.
    pub fn add(&mut self, path: impl Into<String>) {
        let path = path.into();
        self.data.recents.retain(|p| *p != path);
        self.data.recents.insert(0, path);
        let _ = self.save();
    }

    /// Add `path` only if file history is enabled in `settings`
    /// (Python `FileService.add_to_recents`). Returns whether it was added.
    pub fn add_if_enabled(&mut self, path: impl Into<String>, settings: &Settings) -> bool {
        if Self::is_enabled(settings) {
            let path = path.into();
            tracing::info!("Added to recents: {path}");
            self.add(path);
            true
        } else {
            false
        }
    }

    /// Remove `path`; saves only if it was present.
    pub fn remove(&mut self, path: &str) {
        let before = self.data.recents.len();
        self.data.recents.retain(|p| p != path);
        if self.data.recents.len() != before {
            let _ = self.save();
        }
    }

    /// Clear the list and save.
    pub fn clear(&mut self) {
        self.data.recents.clear();
        let _ = self.save();
    }

    /// Whether file history saving is enabled.
    pub fn is_enabled(settings: &Settings) -> bool {
        settings.save_file_history_enabled()
    }
}
