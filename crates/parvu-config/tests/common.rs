//! Shared fixtures (port of the relevant parts of tests/conftest.py).
#![allow(dead_code)]

use parvu_config::{AppPaths, RecentsManager, Settings, ThemeManager};
use tempfile::TempDir;

/// A temp `.ParVu` folder (`temp_settings` fixture). Keep the `TempDir` alive.
pub fn temp_app_dir() -> (TempDir, AppPaths) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let paths = AppPaths::new(tmp.path().join(".ParVu"));
    std::fs::create_dir_all(paths.base_dir()).unwrap();
    (tmp, paths)
}

/// `recents_manager` fixture.
pub fn recents_manager() -> (TempDir, AppPaths, RecentsManager, Settings) {
    let (tmp, paths) = temp_app_dir();
    let mgr = RecentsManager::with_paths(&paths);
    (tmp, paths, mgr, Settings::default())
}

/// `theme_manager` fixture, on a temp folder.
pub fn theme_manager() -> (TempDir, ThemeManager) {
    let (tmp, paths) = temp_app_dir();
    let mgr = ThemeManager::with_paths(&paths);
    (tmp, mgr)
}
