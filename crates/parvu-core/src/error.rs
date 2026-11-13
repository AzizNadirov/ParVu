use serde::{Serialize, Serializer};

/// Errors raised by the core crate.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("{0}")]
    Db(#[from] duckdb::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("Excel error: {0}")]
    Excel(String),
    #[error("Unsupported file format: {0}")]
    UnsupportedFormat(String),
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("No tab with id {0}")]
    NoSuchTab(u64),
    #[error("Unknown column: {0}")]
    UnknownColumn(String),
    #[error("{0}")]
    Invalid(String),
    #[error("Nothing to undo")]
    NothingToUndo,
    #[error("Cancelled")]
    Cancelled,
}

impl CoreError {
    pub fn invalid(msg: impl Into<String>) -> Self {
        CoreError::Invalid(msg.into())
    }
}

/// Errors cross into the UI as their message text.
impl Serialize for CoreError {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T, E = CoreError> = std::result::Result<T, E>;
