//! Themes, ported from `infrastructure/themes`.
//!
//! The serde models match the Python theme JSON, so files in
//! `~/.ParVu/themes` load unchanged. The Qt stylesheet generator is replaced
//! by [`theme_css_variables`], which turns a theme into CSS custom properties
//! for the web UI.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{ConfigError, Result, write_atomic};
use crate::paths::AppPaths;
use crate::settings::Settings;

/// Name of the default theme, used when the configured one is missing.
pub const DEFAULT_THEME_NAME: &str = "Material Light";

/// Colour scheme. Every field defaults to the Python model default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorScheme {
    // Main window
    pub background: String,
    pub foreground: String,
    // Buttons
    pub button_background: String,
    pub button_foreground: String,
    pub button_hover: String,
    pub button_pressed: String,
    // SQL editor
    pub editor_background: String,
    pub editor_foreground: String,
    pub editor_keyword: String,
    pub editor_string: String,
    pub editor_number: String,
    pub editor_comment: String,
    pub editor_selection: String,
    // Table
    pub table_background: String,
    pub table_foreground: String,
    pub table_alternate_row: String,
    pub table_header_background: String,
    pub table_header_foreground: String,
    pub table_grid: String,
    pub table_selection: String,
    // Menu
    pub menu_background: String,
    pub menu_foreground: String,
    pub menu_hover: String,
    // Status bar
    pub status_background: String,
    pub status_foreground: String,
    // Accents
    pub accent_primary: String,
    pub accent_secondary: String,
    pub accent_warning: String,
    pub accent_error: String,
    // Tab bar
    pub tab_active_background: String,
    pub tab_active_border: String,
    pub tab_inactive_background: String,
    pub tab_inactive_border: String,
    pub tab_text_active: String,
    pub tab_text_inactive: String,
    pub tab_close_hover: String,
}

impl Default for ColorScheme {
    fn default() -> Self {
        let s = |v: &str| v.to_owned();
        Self {
            background: s("#FFFFFF"),
            foreground: s("#000000"),
            button_background: s("#E8F5E9"),
            button_foreground: s("#000000"),
            button_hover: s("#C8E6C9"),
            button_pressed: s("#A5D6A7"),
            editor_background: s("#FFF9C4"),
            editor_foreground: s("#000000"),
            editor_keyword: s("#0066CC"),
            editor_string: s("#00AA00"),
            editor_number: s("#AA00AA"),
            editor_comment: s("#808080"),
            editor_selection: s("#B3E5FC"),
            table_background: s("#FFFFFF"),
            table_foreground: s("#000000"),
            table_alternate_row: s("#F5F5F5"),
            table_header_background: s("#E0E0E0"),
            table_header_foreground: s("#000000"),
            table_grid: s("#DDDDDD"),
            table_selection: s("#2196F3"),
            menu_background: s("#FFFFFF"),
            menu_foreground: s("#000000"),
            menu_hover: s("#E3F2FD"),
            status_background: s("#F5F5F5"),
            status_foreground: s("#000000"),
            accent_primary: s("#2196F3"),
            accent_secondary: s("#4CAF50"),
            accent_warning: s("#FF9800"),
            accent_error: s("#F44336"),
            tab_active_background: s("#FFFFFF"),
            tab_active_border: s("#D4D4D4"),
            tab_inactive_background: s("#F3F3F3"),
            tab_inactive_border: s("#D4D4D4"),
            tab_text_active: s("#333333"),
            tab_text_inactive: s("#666666"),
            tab_close_hover: s("#E81123"),
        }
    }
}

/// Layout configuration. Every field defaults to the Python model default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutConfig {
    pub window_min_width: i32,
    pub window_min_height: i32,
    pub sql_editor_height: i32,
    pub toolbar_height: i32,
    pub status_bar_height: i32,
    pub margin: i32,
    pub spacing: i32,
    pub default_font_family: String,
    pub default_font_size: i32,
    pub code_font_family: String,
    pub code_font_size: i32,
    pub table_font_family: String,
    pub table_font_size: i32,
    pub table_row_height: i32,
    pub table_header_height: i32,
    pub show_grid: bool,
    pub alternate_row_colors: bool,
    pub button_min_width: i32,
    pub button_height: i32,
    pub button_border_radius: i32,
    pub show_file_path_label: bool,
    pub show_table_info_button: bool,
    pub show_reset_button: bool,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            window_min_width: 1200,
            window_min_height: 800,
            sql_editor_height: 100,
            toolbar_height: 40,
            status_bar_height: 25,
            margin: 10,
            spacing: 5,
            default_font_family: "Arial".into(),
            default_font_size: 10,
            code_font_family: "Courier New".into(),
            code_font_size: 10,
            table_font_family: "Courier".into(),
            table_font_size: 9,
            table_row_height: 25,
            table_header_height: 30,
            show_grid: true,
            alternate_row_colors: true,
            button_min_width: 64,
            button_height: 28,
            button_border_radius: 8,
            show_file_path_label: true,
            show_table_info_button: true,
            show_reset_button: true,
        }
    }
}

fn default_author() -> String {
    "ParVu".into()
}

fn default_version() -> String {
    "1.0".into()
}

/// A complete theme. `name` is required; everything else has defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_author")]
    pub author: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub colors: ColorScheme,
    #[serde(default)]
    pub layout: LayoutConfig,
}

impl Theme {
    /// A theme with the given name and default colours and layout.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            author: default_author(),
            version: default_version(),
            colors: ColorScheme::default(),
            layout: LayoutConfig::default(),
        }
    }

    /// Pretty JSON, same shape as Python `Theme.to_json`.
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(text: &str) -> std::result::Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    pub fn load_from_file(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| ConfigError::io(path, e))?;
        Self::from_json(&text).map_err(|e| ConfigError::json(path, e))
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        write_atomic(path, &self.to_json()?)?;
        tracing::info!("Theme saved to {}", path.display());
        Ok(())
    }

    /// CSS custom properties for this theme; see [`theme_css_variables`].
    pub fn css_variables(&self) -> String {
        theme_css_variables(self)
    }

    /// File name used in the themes folder: lower-case, spaces → `_`, `.json`.
    pub fn file_name(name: &str) -> String {
        format!("{}.json", name.to_lowercase().replace(' ', "_"))
    }
}

fn builtin(json: &str) -> Theme {
    Theme::from_json(json).expect("built-in theme JSON is valid (checked by tests)")
}

/// The built-in themes: "Material Light", "Material Dark", "ParVu Light",
/// "Excel", "ParVu Black".
pub fn builtin_themes() -> Vec<Theme> {
    vec![
        builtin(include_str!("../themes/material_light.json")),
        builtin(include_str!("../themes/material_dark.json")),
        builtin(include_str!("../themes/parvu_light.json")),
        builtin(include_str!("../themes/excel.json")),
        builtin(include_str!("../themes/parvu_black.json")),
    ]
}

fn kebab(s: &str) -> String {
    s.replace('_', "-").to_lowercase()
}

/// Keep a value from a (possibly hand-edited) theme file from breaking out
/// of its declaration.
fn css_safe(v: &str) -> String {
    v.chars().filter(|c| !matches!(c, ';' | '{' | '}' | '<' | '>' | '\n' | '\r')).collect()
}

fn css_string(v: &str) -> String {
    let escaped = css_safe(v).replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// Text colour for `color` (`#RGB` or `#RRGGBB`): white while it keeps a
/// contrast of at least 3:1 (Material's bar for buttons), else black. White
/// when the colour cannot be parsed.
pub fn on_color(color: &str) -> &'static str {
    let hex = color.trim().trim_start_matches('#');
    let expanded: String = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect(),
        6 => hex.to_owned(),
        _ => return "#FFFFFF",
    };
    let Ok(v) = u32::from_str_radix(&expanded, 16) else { return "#FFFFFF" };
    let lin = |c: u32| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let l = 0.2126 * lin((v >> 16) & 255) + 0.7152 * lin((v >> 8) & 255) + 0.0722 * lin(v & 255);
    if 1.05 / (l + 0.05) >= 3.0 { "#FFFFFF" } else { "#000000" }
}

/// CSS custom properties for every colour and layout value of `theme`.
///
/// Naming: `--pv-<group>-<field>`, where `<group>` is the JSON group
/// (`colors` or `layout`) and `<field>` is the JSON field name with `_`
/// replaced by `-`. Values:
///
/// * colours: as written in the theme (`#RRGGBB`, colour names, ...);
/// * `*_font_size`: `pt` (Qt used points);
/// * other numbers: `px`;
/// * font families: quoted CSS strings;
/// * booleans: `1` or `0`.
///
/// Derived values: `--pv-derived-table-grid` (grid colour or `transparent`
/// when `show_grid` is off), `--pv-derived-table-alternate-row` (alternate
/// row colour, or the table background when `alternate_row_colors` is off)
/// and `--pv-derived-on-accent` (black or white, whichever reads better on
/// `accent_primary`). Fields within a group are emitted in alphabetical order.
pub fn theme_css_variables(theme: &Theme) -> String {
    let mut out = String::from(":root {\n");
    for (group, value) in [
        ("colors", serde_json::to_value(&theme.colors)),
        ("layout", serde_json::to_value(&theme.layout)),
    ] {
        let Ok(Value::Object(fields)) = value else { continue };
        for (field, v) in fields {
            let css = match v {
                Value::String(s) if group == "layout" => css_string(&s),
                Value::String(s) => css_safe(&s),
                Value::Bool(b) => if b { "1" } else { "0" }.to_owned(),
                Value::Number(n) if field.ends_with("_font_size") => format!("{n}pt"),
                Value::Number(n) => format!("{n}px"),
                other => css_safe(&other.to_string()),
            };
            let _ = writeln!(out, "  --pv-{group}-{}: {css};", kebab(&field));
        }
    }
    let c = &theme.colors;
    let l = &theme.layout;
    let grid = if l.show_grid { css_safe(&c.table_grid) } else { "transparent".into() };
    let alt = if l.alternate_row_colors { &c.table_alternate_row } else { &c.table_background };
    let _ = writeln!(out, "  --pv-derived-table-grid: {grid};");
    let _ = writeln!(out, "  --pv-derived-table-alternate-row: {};", css_safe(alt));
    let _ = writeln!(out, "  --pv-derived-on-accent: {};", on_color(&c.accent_primary));
    out.push_str("}\n");
    out
}

/// Built-in and custom themes, and the current selection.
#[derive(Debug, Clone)]
pub struct ThemeManager {
    themes_dir: PathBuf,
    builtin: Vec<Theme>,
    current: Theme,
}

impl Default for ThemeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ThemeManager {
    /// Use `~/.ParVu/themes`.
    pub fn new() -> Self {
        Self::with_paths(&AppPaths::default())
    }

    /// Use `<base>/themes`.
    pub fn with_base_dir(base: impl AsRef<Path>) -> Self {
        Self::with_paths(&AppPaths::new(base.as_ref()))
    }

    pub fn with_paths(paths: &AppPaths) -> Self {
        Self::with_themes_dir(paths.themes_dir())
    }

    /// Use an explicit themes folder. Creates it and writes the built-in
    /// themes there for reference if their files are missing (as Python
    /// does). Failures are logged, not fatal.
    pub fn with_themes_dir(themes_dir: impl Into<PathBuf>) -> Self {
        let themes_dir = themes_dir.into();
        let builtin = builtin_themes();
        if let Err(e) = std::fs::create_dir_all(&themes_dir) {
            tracing::warn!("Cannot create themes folder {}: {e}", themes_dir.display());
        } else {
            for theme in &builtin {
                let file = themes_dir.join(Theme::file_name(&theme.name));
                if !file.exists()
                    && let Err(e) = theme.save_to_file(&file)
                {
                    tracing::warn!("Cannot write built-in theme {}: {e}", theme.name);
                }
            }
        }
        let current = builtin[0].clone();
        tracing::info!("ThemeManager initialized with {} built-in themes", builtin.len());
        Self { themes_dir, builtin, current }
    }

    pub fn themes_dir(&self) -> &Path {
        &self.themes_dir
    }

    pub fn builtin_themes(&self) -> &[Theme] {
        &self.builtin
    }

    pub fn is_builtin(&self, name: &str) -> bool {
        self.builtin.iter().any(|t| t.name == name)
    }

    /// Theme by name: built-ins first, then `<themes_dir>/<slug>.json`.
    pub fn get_theme(&self, name: &str) -> Option<Theme> {
        if let Some(t) = self.builtin.iter().find(|t| t.name == name) {
            return Some(t.clone());
        }
        let file = self.themes_dir.join(Theme::file_name(name));
        if !file.exists() {
            return None;
        }
        match Theme::load_from_file(&file) {
            Ok(t) => Some(t),
            Err(e) => {
                tracing::error!("Failed to load theme {name}: {e}");
                None
            }
        }
    }

    /// Custom (non built-in) themes found in the themes folder. Unreadable
    /// files are skipped.
    pub fn custom_themes(&self) -> Vec<Theme> {
        let Ok(entries) = std::fs::read_dir(&self.themes_dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        files.sort();
        let mut out: Vec<Theme> = Vec::new();
        for file in files {
            match Theme::load_from_file(&file) {
                Ok(t) if !self.is_builtin(&t.name) && !out.iter().any(|o| o.name == t.name) => out.push(t),
                Ok(_) => {}
                Err(e) => tracing::debug!("Skipping theme file {}: {e}", file.display()),
            }
        }
        out
    }

    /// Names of all built-in and custom themes, sorted.
    pub fn list_themes(&self) -> Vec<String> {
        let mut names: Vec<String> = self.builtin.iter().map(|t| t.name.clone()).collect();
        names.extend(self.custom_themes().into_iter().map(|t| t.name));
        names.sort();
        names
    }

    /// Current theme (the default theme until one is set).
    pub fn current_theme(&self) -> &Theme {
        &self.current
    }

    /// Select a theme by name. Returns `false` and keeps the current theme
    /// if it does not exist.
    pub fn set_theme(&mut self, name: &str) -> bool {
        match self.get_theme(name) {
            Some(t) => {
                self.current = t;
                tracing::info!("Theme set to: {name}");
                true
            }
            None => false,
        }
    }

    /// Select `settings.current_theme`, falling back to the default theme if it
    /// does not exist.
    pub fn apply_settings(&mut self, settings: &Settings) -> &Theme {
        if !self.set_theme(&settings.current_theme) {
            tracing::warn!(
                "Theme {:?} not found; using {DEFAULT_THEME_NAME}",
                settings.current_theme
            );
            self.set_theme(DEFAULT_THEME_NAME);
        }
        &self.current
    }

    /// Write a theme to `path`.
    pub fn export_theme(&self, name: &str, path: &Path) -> Result<()> {
        let theme = self.get_theme(name).ok_or_else(|| ConfigError::ThemeNotFound(name.into()))?;
        theme.save_to_file(path)
    }

    /// Copy a theme file into the themes folder; returns its name.
    pub fn import_theme(&self, path: &Path) -> Result<String> {
        let theme = Theme::load_from_file(path)?;
        theme.save_to_file(&self.themes_dir.join(Theme::file_name(&theme.name)))?;
        tracing::info!("Imported theme: {}", theme.name);
        Ok(theme.name)
    }

    /// Delete a custom theme. Built-in themes cannot be deleted.
    pub fn delete_theme(&self, name: &str) -> Result<()> {
        if self.is_builtin(name) {
            tracing::warn!("Cannot delete built-in theme: {name}");
            return Err(ConfigError::BuiltinTheme(name.into()));
        }
        let file = self.themes_dir.join(Theme::file_name(name));
        if !file.exists() {
            return Err(ConfigError::ThemeNotFound(name.into()));
        }
        std::fs::remove_file(&file).map_err(|e| ConfigError::io(&file, e))?;
        tracing::info!("Deleted theme: {name}");
        Ok(())
    }

    /// CSS variables for the current theme.
    pub fn css_variables(&self) -> String {
        theme_css_variables(&self.current)
    }
}
