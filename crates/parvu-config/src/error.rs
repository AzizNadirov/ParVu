//! Error type for the crate.

use std::path::PathBuf;

/// Errors raised by configuration, recents and theme operations.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("JSON serialisation failed: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("theme not found: {0}")]
    ThemeNotFound(String),
    #[error("built-in theme cannot be modified: {0}")]
    BuiltinTheme(String),
}

pub type Result<T, E = ConfigError> = std::result::Result<T, E>;

impl ConfigError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }

    pub(crate) fn json(path: impl Into<PathBuf>, source: serde_json::Error) -> Self {
        Self::Json { path: path.into(), source }
    }
}

/// Write `contents` to `path` via a sibling temp file and rename, creating
/// parent directories first, so a crash never leaves a half-written file.
pub(crate) fn write_atomic(path: &std::path::Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ConfigError::io(parent, e))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, contents).map_err(|e| ConfigError::io(&tmp, e))?;
    std::fs::rename(&tmp, path).map_err(|e| ConfigError::io(path, e))
}
