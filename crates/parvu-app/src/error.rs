//! The single error type every command returns. It reaches the UI as
//! `{ kind, message }`, plus `line`/`column` for expression parse errors.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub kind: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
}

impl ApiError {
    pub fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), line: None, column: None }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<parvu_core::CoreError> for ApiError {
    fn from(e: parvu_core::CoreError) -> Self {
        use parvu_core::CoreError as E;
        let kind = match e {
            E::Db(_) => "query",
            E::FileNotFound(_) => "file_not_found",
            E::UnsupportedFormat(_) => "unsupported_format",
            E::NothingToUndo => "nothing_to_undo",
            E::Cancelled => "cancelled",
            _ => "core",
        };
        ApiError::new(kind, e.to_string())
    }
}

impl From<parvu_dsl::DslError> for ApiError {
    fn from(e: parvu_dsl::DslError) -> Self {
        match e {
            parvu_dsl::DslError::Parse { message, line, column } => {
                ApiError { kind: "expression", message, line: Some(line), column: Some(column) }
            }
            other => ApiError::new("expression", other.to_string()),
        }
    }
}

impl From<parvu_config::ConfigError> for ApiError {
    fn from(e: parvu_config::ConfigError) -> Self {
        ApiError::new("config", e.to_string())
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        ApiError::new("io", e.to_string())
    }
}

impl From<tauri::Error> for ApiError {
    fn from(e: tauri::Error) -> Self {
        ApiError::new("app", e.to_string())
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
