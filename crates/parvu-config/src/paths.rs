//! Path helpers, ported from `infrastructure/paths.py`.
//!
//! ParVu keeps user data in the legacy `~/.ParVu` folder. The XDG helpers are
//! ported too, for a possible later move.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

const APP_DIR_NAME: &str = "ParVu";

/// The user's home directory, or the current directory if it cannot be found.
pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Legacy app directory (`~/.ParVu`). All managers default to this.
pub fn legacy_app_dir() -> PathBuf {
    home_dir().join(".ParVu")
}

fn xdg_dir(var: Option<OsString>, fallback: &[&str]) -> PathBuf {
    match var {
        Some(v) if !v.is_empty() => PathBuf::from(v).join(APP_DIR_NAME),
        _ => fallback.iter().fold(home_dir(), |p, s| p.join(s)).join(APP_DIR_NAME),
    }
}

/// `$XDG_DATA_HOME/ParVu`, or `~/.local/share/ParVu`.
pub fn user_data_dir() -> PathBuf {
    xdg_dir(std::env::var_os("XDG_DATA_HOME"), &[".local", "share"])
}

/// `$XDG_CONFIG_HOME/ParVu`, or `~/.config/ParVu`.
pub fn user_config_dir() -> PathBuf {
    xdg_dir(std::env::var_os("XDG_CONFIG_HOME"), &[".config"])
}

/// `$XDG_CACHE_HOME/ParVu`, or `~/.cache/ParVu`.
pub fn user_cache_dir() -> PathBuf {
    xdg_dir(std::env::var_os("XDG_CACHE_HOME"), &[".cache"])
}

/// Every file and folder ParVu uses under one base directory
/// (`~/.ParVu` by default).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    base: PathBuf,
}

impl Default for AppPaths {
    fn default() -> Self {
        Self::new(legacy_app_dir())
    }
}

impl AppPaths {
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    /// The base directory (`~/.ParVu`).
    pub fn base_dir(&self) -> &Path {
        &self.base
    }

    pub fn settings_dir(&self) -> PathBuf {
        self.base.join("settings")
    }

    /// `settings/settings.json`
    pub fn settings_file(&self) -> PathBuf {
        self.settings_dir().join("settings.json")
    }

    /// `settings/default_settings.json`
    pub fn default_settings_file(&self) -> PathBuf {
        self.settings_dir().join("default_settings.json")
    }

    pub fn history_dir(&self) -> PathBuf {
        self.base.join("history")
    }

    /// `history/recents.json`
    pub fn recents_file(&self) -> PathBuf {
        self.history_dir().join("recents.json")
    }

    /// `logs/` (not created).
    pub fn logs_dir(&self) -> PathBuf {
        self.base.join("logs")
    }

    /// `themes/` (not created).
    pub fn themes_dir(&self) -> PathBuf {
        self.base.join("themes")
    }

    /// `logs/`, created if missing (like Python `get_log_dir`).
    pub fn ensure_logs_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.logs_dir();
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// `themes/`, created if missing (like Python `get_themes_dir`).
    pub fn ensure_themes_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.themes_dir();
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}

/// `~/.ParVu/logs`, created if missing.
pub fn log_dir() -> std::io::Result<PathBuf> {
    AppPaths::default().ensure_logs_dir()
}

/// `~/.ParVu/themes`, created if missing.
pub fn themes_dir() -> std::io::Result<PathBuf> {
    AppPaths::default().ensure_themes_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xdg_override_and_fallback() {
        assert_eq!(
            xdg_dir(Some("/xdg/data".into()), &[".local", "share"]),
            PathBuf::from("/xdg/data/ParVu")
        );
        assert_eq!(
            xdg_dir(None, &[".local", "share"]),
            home_dir().join(".local/share/ParVu")
        );
        assert_eq!(xdg_dir(Some("".into()), &[".cache"]), home_dir().join(".cache/ParVu"));
    }

    #[test]
    fn layout_matches_python() {
        let p = AppPaths::new("/b");
        assert_eq!(p.settings_file(), PathBuf::from("/b/settings/settings.json"));
        assert_eq!(p.default_settings_file(), PathBuf::from("/b/settings/default_settings.json"));
        assert_eq!(p.recents_file(), PathBuf::from("/b/history/recents.json"));
        assert_eq!(p.logs_dir(), PathBuf::from("/b/logs"));
        assert_eq!(p.themes_dir(), PathBuf::from("/b/themes"));
        assert_eq!(legacy_app_dir(), home_dir().join(".ParVu"));
    }
}
