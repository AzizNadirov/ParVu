//! Runtime application state, ported from `config/app_state.py`.
//! Not persisted. The cached pandas DataFrame of the Python version is not
//! ported; page data lives with the query engine.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppState {
    pub current_file: Option<PathBuf>,
    pub current_page: u64,
    pub current_query: String,
    pub is_loading: bool,
    pub status_message: String,
    pub current_theme_name: String,
    pub session_id: String,
    pub log_file: Option<PathBuf>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            current_file: None,
            current_page: 1,
            current_query: String::new(),
            is_loading: false,
            status_message: "Ready".into(),
            current_theme_name: "ParVu Light".into(),
            session_id: String::new(),
            log_file: None,
        }
    }
}

impl AppState {
    /// Reset file/query/UI state. Theme and session are kept, as in Python.
    pub fn reset(&mut self) {
        self.current_file = None;
        self.current_page = 1;
        self.current_query.clear();
        self.is_loading = false;
        self.status_message = "Ready".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_keeps_theme_and_session() {
        let mut s = AppState {
            current_file: Some("/x.parquet".into()),
            current_page: 4,
            current_query: "q".into(),
            is_loading: true,
            status_message: "busy".into(),
            current_theme_name: "Excel".into(),
            session_id: "abc".into(),
            log_file: None,
        };
        s.reset();
        assert_eq!(s.current_file, None);
        assert_eq!(s.current_page, 1);
        assert_eq!(s.status_message, "Ready");
        assert_eq!(s.current_theme_name, "Excel");
        assert_eq!(s.session_id, "abc");
    }
}
