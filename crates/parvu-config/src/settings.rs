//! Application settings, ported from `config/settings.py`.
//!
//! The JSON keys match the Python `Settings` model exactly, so an existing
//! `~/.ParVu/settings/settings.json` loads unchanged. Loading is lenient:
//!
//! * unknown keys are kept (and written back on save), so files written by
//!   the Python app round-trip, including its path fields;
//! * a key whose value has the wrong type falls back to its default with a
//!   warning instead of failing the whole file;
//! * a file that is not valid JSON at all is reset to the bundled defaults.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::path::Path;

use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::error::{ConfigError, Result, write_atomic};
use crate::paths::AppPaths;

/// Bundled `resources/settings/settings.json` (copied to the user folder on reset).
pub const BUNDLED_SETTINGS_JSON: &str = include_str!("../resources/settings.json");
/// Bundled `resources/settings/default_settings.json`.
pub const BUNDLED_DEFAULT_SETTINGS_JSON: &str = include_str!("../resources/default_settings.json");
/// Bundled `resources/history/recents.json`.
pub const BUNDLED_RECENTS_JSON: &str = include_str!("../resources/recents.json");

/// A Python `int | str` field. Old files store numbers as strings such as
/// `"500"`; both forms are accepted and written back in the form they came in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntOrString {
    Int(i64),
    Str(String),
}

impl IntOrString {
    /// The value as an integer, if it is one or is a string holding one.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            Self::Str(s) => {
                let s = s.trim();
                s.parse::<i64>().ok().or_else(|| {
                    s.parse::<f64>()
                        .ok()
                        .filter(|f| f.is_finite() && f.fract() == 0.0)
                        .map(|f| f as i64)
                })
            }
        }
    }

    /// The value as a non-negative integer, or `default` if it is not one.
    pub fn as_u64_or(&self, default: u64) -> u64 {
        self.as_i64()
            .and_then(|v| u64::try_from(v).ok())
            .unwrap_or(default)
    }
}

impl From<i64> for IntOrString {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}

impl From<&str> for IntOrString {
    fn from(v: &str) -> Self {
        Self::Str(v.to_owned())
    }
}

impl From<String> for IntOrString {
    fn from(v: String) -> Self {
        Self::Str(v)
    }
}

/// Same as Python `str(value)`.
impl fmt::Display for IntOrString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(i) => write!(f, "{i}"),
            Self::Str(s) => f.write_str(s),
        }
    }
}

impl Serialize for IntOrString {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Self::Int(i) => s.serialize_i64(*i),
            Self::Str(v) => s.serialize_str(v),
        }
    }
}

impl<'de> Deserialize<'de> for IntOrString {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = IntOrString;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an integer or a string")
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Self::Value, E> {
                Ok(IntOrString::Int(v))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Self::Value, E> {
                i64::try_from(v)
                    .map(IntOrString::Int)
                    .map_err(|_| E::custom("integer out of range"))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Self::Value, E> {
                // Pydantic accepts floats with no fractional part for int fields.
                if v.is_finite() && v.fract() == 0.0 && v.abs() < 9.0e15 {
                    Ok(IntOrString::Int(v as i64))
                } else {
                    Err(E::custom("expected an integer"))
                }
            }
            fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Self::Value, E> {
                Ok(IntOrString::Str(v.to_owned()))
            }
        }
        d.deserialize_any(V)
    }
}

/// `warning_criteria` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WarningCriteria {
    #[default]
    Rows,
    Cells,
    Filesize,
}

/// Application settings. Field names are the JSON keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // Data settings
    pub default_data_var_name: String,
    pub default_limit: IntOrString,
    pub default_sql_font_size: IntOrString,
    pub default_result_font_size: IntOrString,
    pub default_sql_query: String,
    pub default_sql_font: String,
    pub sql_keywords: Vec<String>,
    pub result_pagination_rows_per_page: IntOrString,
    pub save_file_history: String,
    pub max_rows: IntOrString,

    // Theme and language
    pub current_theme: String,
    pub current_language: String,

    // Large dataset warning
    pub enable_large_dataset_warning: bool,
    pub warning_criteria: WarningCriteria,

    // Exit warning
    pub warn_on_exit_with_transforms: bool,

    pub copy_tuple_sample_size: i64,
    pub warning_threshold_rows: i64,
    pub warning_threshold_cells: i64,
    pub warning_threshold_filesize_mb: i64,

    // Crash reporting
    pub bug_report_email: String,
    pub enable_crash_reporting: bool,

    // Legacy colours, kept for backward compatibility
    #[serde(rename = "colour_browseButton")]
    pub colour_browse_button: String,
    #[serde(rename = "colour_sqlEdit")]
    pub colour_sql_edit: String,
    #[serde(rename = "colour_executeButton")]
    pub colour_execute_button: String,
    #[serde(rename = "colour_resultTable")]
    pub colour_result_table: String,
    #[serde(rename = "colour_tableInfoButton")]
    pub colour_table_info_button: String,

    /// Keys this model does not know, kept so they survive a save. Files
    /// written by the Python app hold path fields here (`usr_settings_file`,
    /// `static_dir`, ...); they are not used, paths come from [`AppPaths`].
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for Settings {
    /// The Python model defaults (not the bundled settings.json values).
    fn default() -> Self {
        Self {
            default_data_var_name: "data".into(),
            default_limit: IntOrString::Int(1000),
            default_sql_font_size: IntOrString::Int(12),
            default_result_font_size: IntOrString::Int(10),
            default_sql_query: "SELECT * FROM $(default_data_var_name)".into(),
            default_sql_font: "Courier New".into(),
            sql_keywords: Vec::new(),
            result_pagination_rows_per_page: IntOrString::Int(100),
            save_file_history: "True".into(),
            max_rows: IntOrString::Int(100_000),
            current_theme: "ParVu Light".into(),
            current_language: "en".into(),
            enable_large_dataset_warning: true,
            warning_criteria: WarningCriteria::Rows,
            warn_on_exit_with_transforms: true,
            copy_tuple_sample_size: 500,
            warning_threshold_rows: 1_000_000,
            warning_threshold_cells: 10_000_000,
            warning_threshold_filesize_mb: 100,
            bug_report_email: "eziznadirov@gmail.com".into(),
            enable_crash_reporting: true,
            colour_browse_button: "#E8F5E9".into(),
            colour_sql_edit: "#FFF9C4".into(),
            colour_execute_button: "#E1F5FE".into(),
            colour_result_table: "#FFFFFF".into(),
            colour_table_info_button: "#F3E5F5".into(),
            extra: BTreeMap::new(),
        }
    }
}

impl Settings {
    /// Parse settings from JSON. Fails only if the text is not a JSON object;
    /// keys with values of the wrong type are dropped (default used) and
    /// logged.
    pub fn from_json_lenient(text: &str) -> std::result::Result<Self, serde_json::Error> {
        let value: Value = serde_json::from_str(text)?;
        let Value::Object(input) = value else {
            return Err(de::Error::custom("settings must be a JSON object"));
        };
        Ok(Self::from_map_lenient(input))
    }

    fn from_map_lenient(input: Map<String, Value>) -> Self {
        // Fast path: everything valid.
        if let Ok(s) = serde_json::from_value::<Settings>(Value::Object(input.clone())) {
            return s.normalized();
        }
        // Add keys one at a time and drop any that break deserialisation.
        let mut accepted = Map::new();
        for (key, value) in input {
            let mut candidate = accepted.clone();
            candidate.insert(key.clone(), value.clone());
            match serde_json::from_value::<Settings>(Value::Object(candidate)) {
                Ok(_) => {
                    accepted.insert(key, value);
                }
                Err(e) => {
                    tracing::warn!("Ignoring invalid setting {key:?} = {value}: {e}; using default");
                }
            }
        }
        serde_json::from_value::<Settings>(Value::Object(accepted))
            .unwrap_or_default()
            .normalized()
    }

    /// Python `model_post_init`: upper-case, trim and de-duplicate
    /// `sql_keywords` (first occurrence wins; Python's set order was arbitrary).
    fn normalized(mut self) -> Self {
        let mut seen = HashSet::new();
        self.sql_keywords = std::mem::take(&mut self.sql_keywords)
            .into_iter()
            .map(|k| k.trim().to_uppercase())
            .filter(|k| seen.insert(k.clone()))
            .collect();
        self
    }

    /// Pretty JSON (2-space indent) as written to `settings.json`.
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Replace `$(name)` template variables in a query, like Python `render_vars`.
    pub fn render_vars(&self, query: &str) -> String {
        query
            .replace("$(default_data_var_name)", &self.default_data_var_name)
            .replace("$(default_limit)", &self.default_limit.to_string())
            .replace("$(default_sql_font_size)", &self.default_sql_font_size.to_string())
            .replace("$(default_sql_query)", &self.default_sql_query)
            .replace("$(default_sql_font)", &self.default_sql_font)
    }

    /// `default_limit` as a number (1000 if unparsable).
    pub fn default_limit(&self) -> u64 {
        self.default_limit.as_u64_or(1000)
    }

    /// `default_sql_font_size` as a number (12 if unparsable).
    pub fn default_sql_font_size(&self) -> u32 {
        clamp_u32(self.default_sql_font_size.as_u64_or(12))
    }

    /// `default_result_font_size` as a number (10 if unparsable).
    pub fn default_result_font_size(&self) -> u32 {
        clamp_u32(self.default_result_font_size.as_u64_or(10))
    }

    /// `result_pagination_rows_per_page` as a number (100 if unparsable or 0).
    pub fn rows_per_page(&self) -> u64 {
        match self.result_pagination_rows_per_page.as_u64_or(100) {
            0 => 100,
            n => n,
        }
    }

    /// `max_rows` as a number (100000 if unparsable).
    pub fn max_rows(&self) -> u64 {
        self.max_rows.as_u64_or(100_000)
    }

    /// Whether recent files are recorded (Python `RecentsManager.is_enabled`:
    /// `"True"`, `"true"` or `"1"`).
    pub fn save_file_history_enabled(&self) -> bool {
        matches!(self.save_file_history.as_str(), "True" | "true" | "1")
    }
}

fn clamp_u32(v: u64) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

/// Where [`SettingsManager::reset_with`] takes its defaults from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultsSource {
    /// The bundled resource files (embedded in the binary).
    Bundled,
    /// [`Settings::default()`] and an empty recents list. Used when the
    /// bundled files are unusable, like the Python fallback for a broken
    /// bundle.
    BuiltIn,
}

/// Loads, resets and saves [`Settings`].
#[derive(Debug, Clone)]
pub struct SettingsManager {
    paths: AppPaths,
    settings: Settings,
}

impl Default for SettingsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsManager {
    /// Load from `~/.ParVu`.
    pub fn new() -> Self {
        Self::with_paths(AppPaths::default())
    }

    /// Load from `base` (a `.ParVu`-style folder).
    pub fn with_base_dir(base: impl AsRef<Path>) -> Self {
        Self::with_paths(AppPaths::new(base.as_ref()))
    }

    /// Load from the given paths.
    pub fn with_paths(paths: AppPaths) -> Self {
        let settings = Self::load(&paths);
        Self { paths, settings }
    }

    /// Wrap already-built settings; nothing is read from disk.
    pub fn from_settings(paths: AppPaths, settings: Settings) -> Self {
        Self { paths, settings }
    }

    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }

    /// Load settings, creating defaults on first run. Never fails: a missing
    /// or corrupt file is logged and reset to defaults.
    pub fn load(paths: &AppPaths) -> Settings {
        // A missing settings file is a first run (the logs folder may already
        // exist), not an error.
        if !paths.settings_file().exists() {
            tracing::info!("No settings file at {}; creating defaults", paths.settings_file().display());
            if let Err(e) = Self::reset(paths) {
                tracing::error!("Failed to create default settings: {e}");
            }
        }

        match Self::read(paths) {
            Ok(s) => {
                tracing::info!("Settings loaded from user directory");
                s
            }
            Err(e) => {
                tracing::error!("Failed to load settings: {e}");
                tracing::error!("Resetting user settings");
                if let Err(e) = Self::reset(paths) {
                    tracing::error!("Failed to reset settings: {e}");
                }
                Self::read(paths).unwrap_or_else(|e| {
                    tracing::error!("Settings still unreadable after reset ({e}); using built-in defaults");
                    Settings::default()
                })
            }
        }
    }

    fn read(paths: &AppPaths) -> Result<Settings> {
        let file = paths.settings_file();
        let text = std::fs::read_to_string(&file).map_err(|e| ConfigError::io(&file, e))?;
        Settings::from_json_lenient(&text).map_err(|e| ConfigError::json(&file, e))
    }

    /// Reset the user settings to the bundled defaults (Python `reset`).
    pub fn reset(paths: &AppPaths) -> Result<()> {
        Self::reset_with(paths, DefaultsSource::Bundled)
    }

    /// Reset from the given source. Overwrites `settings.json` and
    /// `default_settings.json`. `recents.json` is only written when it is
    /// missing or unreadable, so a reset does not wipe the recent files list.
    pub fn reset_with(paths: &AppPaths, source: DefaultsSource) -> Result<()> {
        let source = match source {
            DefaultsSource::Bundled if Settings::from_json_lenient(BUNDLED_SETTINGS_JSON).is_err() => {
                tracing::warn!("Bundled settings are invalid; using built-in defaults");
                DefaultsSource::BuiltIn
            }
            s => s,
        };

        let (settings_json, default_json, recents_json) = match source {
            DefaultsSource::Bundled => (
                BUNDLED_SETTINGS_JSON.to_owned(),
                BUNDLED_DEFAULT_SETTINGS_JSON.to_owned(),
                BUNDLED_RECENTS_JSON.to_owned(),
            ),
            DefaultsSource::BuiltIn => {
                let defaults = Settings::default().to_json()?;
                (defaults.clone(), defaults, r#"{"recents": []}"#.to_owned())
            }
        };

        write_atomic(&paths.settings_file(), &settings_json)?;
        write_atomic(&paths.default_settings_file(), &default_json)?;

        let recents = paths.recents_file();
        let recents_ok = std::fs::read_to_string(&recents)
            .ok()
            .and_then(|t| serde_json::from_str::<crate::recents::RecentsData>(&t).ok())
            .is_some();
        if !recents_ok {
            write_atomic(&recents, &recents_json)?;
        }

        tracing::info!("User settings reset to defaults");
        Ok(())
    }

    /// Write the current settings to `settings/settings.json`.
    pub fn save(&self) -> Result<()> {
        let file = self.paths.settings_file();
        let result = self.settings.to_json().and_then(|json| write_atomic(&file, &json));
        match &result {
            Ok(()) => tracing::info!("Settings saved"),
            Err(e) => tracing::error!("Failed to save settings: {e}"),
        }
        result
    }

    /// Set several values by JSON key and save (Python `update(**kwargs)`).
    /// Unknown keys and values of the wrong type are skipped with a warning
    /// and returned.
    pub fn update(&mut self, values: Map<String, Value>) -> Result<Vec<String>> {
        let mut current = match serde_json::to_value(&self.settings)? {
            Value::Object(m) => m,
            _ => unreachable!("Settings serialises to an object"),
        };
        let known = known_keys();
        let mut rejected = Vec::new();
        for (key, value) in values {
            if !known.contains(&key.as_str()) {
                tracing::warn!("Unknown setting: {key}");
                rejected.push(key);
                continue;
            }
            let mut candidate = current.clone();
            candidate.insert(key.clone(), value);
            match serde_json::from_value::<Settings>(Value::Object(candidate.clone())) {
                Ok(_) => current = candidate,
                Err(e) => {
                    tracing::warn!("Invalid value for setting {key}: {e}");
                    rejected.push(key);
                }
            }
        }
        self.settings = serde_json::from_value::<Settings>(Value::Object(current))?.normalized();
        self.save()?;
        Ok(rejected)
    }
}

/// JSON keys of the [`Settings`] model (excluding `extra`).
fn known_keys() -> Vec<&'static str> {
    vec![
        "default_data_var_name",
        "default_limit",
        "default_sql_font_size",
        "default_result_font_size",
        "default_sql_query",
        "default_sql_font",
        "sql_keywords",
        "result_pagination_rows_per_page",
        "save_file_history",
        "max_rows",
        "current_theme",
        "current_language",
        "enable_large_dataset_warning",
        "warning_criteria",
        "warn_on_exit_with_transforms",
        "copy_tuple_sample_size",
        "warning_threshold_rows",
        "warning_threshold_cells",
        "warning_threshold_filesize_mb",
        "bug_report_email",
        "enable_crash_reporting",
        "colour_browseButton",
        "colour_sqlEdit",
        "colour_executeButton",
        "colour_resultTable",
        "colour_tableInfoButton",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_keys_match_serialised_fields() {
        let Value::Object(m) = serde_json::to_value(Settings::default()).unwrap() else {
            panic!()
        };
        let mut a: Vec<_> = m.keys().map(String::as_str).collect();
        let mut b = known_keys();
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b);
    }
}
