//! Process-wide state shared by every window.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use parvu_config::{I18n, RecentsManager, SettingsManager, ThemeManager};
use parvu_core::Workspace;
use parvu_core::duckdb::InterruptHandle;

use crate::error::{ApiError, ApiResult};

/// Configuration shared by all windows.
pub struct Config {
    pub settings: SettingsManager,
    pub recents: RecentsManager,
    pub themes: ThemeManager,
    pub i18n: I18n,
}

/// A running export that can be cancelled.
pub struct ExportJob {
    pub interrupt: Arc<InterruptHandle>,
    pub cancelled: Arc<AtomicBool>,
}

pub struct AppState {
    pub config: Mutex<Config>,
    /// One workspace (set of tabs) per window label.
    pub workspaces: Mutex<HashMap<String, Arc<Mutex<Workspace>>>>,
    pub exports: Mutex<HashMap<u64, ExportJob>>,
    pub next_job: AtomicU64,
    pub next_window: AtomicU64,
    /// Files passed on the command line, opened by the first window.
    pub initial_files: Mutex<Vec<PathBuf>>,
    /// Text of a crash report left by the previous run, shown once.
    pub pending_crash: Mutex<Option<String>>,
}

/// Lock a mutex, recovering the data if a panicking thread poisoned it.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl AppState {
    pub fn new(config: Config, initial_files: Vec<PathBuf>, pending_crash: Option<String>) -> Self {
        Self {
            config: Mutex::new(config),
            workspaces: Mutex::new(HashMap::new()),
            exports: Mutex::new(HashMap::new()),
            next_job: AtomicU64::new(1),
            next_window: AtomicU64::new(2),
            initial_files: Mutex::new(initial_files),
            pending_crash: Mutex::new(pending_crash),
        }
    }

    /// The workspace of window `label`, created on first use.
    pub fn workspace(&self, label: &str) -> ApiResult<Arc<Mutex<Workspace>>> {
        let mut map = lock(&self.workspaces);
        if let Some(ws) = map.get(label) {
            return Ok(ws.clone());
        }
        let (page_size, memory_mb, temp) = {
            let cfg = lock(&self.config);
            let s = cfg.settings.settings();
            (s.rows_per_page(), s.query_memory_limit_mb, parvu_config::paths::user_cache_dir().join("duckdb"))
        };
        let ws = Workspace::new(page_size, Some(&temp)).map_err(ApiError::from)?;
        // The PARVU_MEMORY_LIMIT_MB environment variable, if set, wins.
        if std::env::var_os("PARVU_MEMORY_LIMIT_MB").is_none() {
            ws.set_memory_limit_mb(memory_mb.max(0) as u64).map_err(ApiError::from)?;
        }
        let ws = Arc::new(Mutex::new(ws));
        map.insert(label.to_string(), ws.clone());
        Ok(ws)
    }

    pub fn drop_workspace(&self, label: &str) {
        lock(&self.workspaces).remove(label);
    }

    pub fn next_job_id(&self) -> u64 {
        self.next_job.fetch_add(1, Ordering::Relaxed)
    }
}
