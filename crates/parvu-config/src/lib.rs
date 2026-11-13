//! Configuration, translation and theme support for ParVu.
//!
//! Reads the same `~/.ParVu` folder as the Python app, with no migration:
//!
//! * `settings/settings.json` → [`Settings`] via [`SettingsManager`]
//! * `history/recents.json` → [`RecentsManager`]
//! * `themes/*.json` → [`Theme`] via [`ThemeManager`]
//!
//! Every manager takes a base directory so tests never touch the real home
//! folder; the `new()` constructors use [`paths::legacy_app_dir`].

pub mod app_state;
pub mod error;
pub mod i18n;
pub mod paths;
pub mod recents;
pub mod settings;
pub mod themes;

pub use app_state::AppState;
pub use error::{ConfigError, Result};
pub use i18n::{FmtArg, I18n, Language};
pub use paths::AppPaths;
pub use recents::{RecentsData, RecentsManager};
pub use settings::{DefaultsSource, IntOrString, Settings, SettingsManager, WarningCriteria};
pub use themes::{ColorScheme, LayoutConfig, Theme, ThemeManager, theme_css_variables};
