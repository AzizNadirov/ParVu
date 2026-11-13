mod common;

use parvu_config::settings::{BUNDLED_DEFAULT_SETTINGS_JSON, BUNDLED_SETTINGS_JSON};
use parvu_config::{AppPaths, DefaultsSource, IntOrString, Settings, SettingsManager, WarningCriteria};
use serde_json::json;

#[test]
fn settings_render_vars() {
    let settings = Settings {
        default_data_var_name: "mydata".into(),
        default_limit: 500.into(),
        default_sql_font_size: 14.into(),
        default_sql_query: "SELECT * FROM $(default_data_var_name) LIMIT $(default_limit)".into(),
        default_sql_font: "Monaco".into(),
        ..Settings::default()
    };
    assert_eq!(settings.render_vars("SELECT * FROM $(default_data_var_name)"), "SELECT * FROM mydata");
    assert_eq!(settings.render_vars("LIMIT $(default_limit)"), "LIMIT 500");
    assert_eq!(
        settings.render_vars("$(default_sql_query) /* $(default_sql_font) $(default_sql_font_size) */"),
        "SELECT * FROM $(default_data_var_name) LIMIT $(default_limit) /* Monaco 14 */"
    );
    // String-typed values render verbatim, like Python str().
    let s = Settings { default_limit: "0500".into(), ..Settings::default() };
    assert_eq!(s.render_vars("$(default_limit)"), "0500");
}

#[test]
fn settings_defaults() {
    let s = Settings::default();
    assert_eq!(s.current_theme, "ParVu Light");
    assert_eq!(s.current_language, "en");
    assert!(s.enable_large_dataset_warning);
    assert_eq!(s.default_limit(), 1000);
    assert_eq!(s.rows_per_page(), 100);
    assert_eq!(s.max_rows(), 100_000);
    assert_eq!(s.warning_criteria, WarningCriteria::Rows);
    // Missing keys take the model defaults.
    assert_eq!(Settings::from_json_lenient("{}").unwrap(), s);
}

#[test]
fn loads_bundled_settings_json() {
    let s = Settings::from_json_lenient(BUNDLED_SETTINGS_JSON).unwrap();
    assert_eq!(s.default_limit, IntOrString::Str("500".into()));
    assert_eq!(s.default_limit(), 500);
    assert_eq!(s.default_sql_font_size(), 10);
    // Keys absent from settings.json take model defaults.
    assert_eq!(s.default_result_font_size(), 10);
    assert_eq!(s.rows_per_page(), 500);
    assert_eq!(s.max_rows(), 1000);
    assert!(s.save_file_history_enabled());
    assert_eq!(s.current_theme, "ParVu Light");
    assert_eq!(s.bug_report_email, "eziznadirov@gmail.com");
    assert_eq!(s.colour_execute_button, "yellow");
    assert!(s.sql_keywords.contains(&"ORDER BY".to_string()));
    assert!(s.extra.is_empty());
    assert_eq!(
        s.render_vars(&s.default_sql_query),
        "SELECT * \nFROM data \nLIMIT 500"
    );
}

#[test]
fn loads_bundled_default_settings_json() {
    let s = Settings::from_json_lenient(BUNDLED_DEFAULT_SETTINGS_JSON).unwrap();
    assert_eq!(s.default_limit(), 500);
    assert_eq!(s.default_result_font_size(), 11);
    assert_eq!(s.current_theme, "ParVu Light");
    assert_eq!(s.bug_report_email, "parvu.bugs@gmail.com");
    assert_eq!(s.colour_table_info_button, "#95baf5");
    assert!(s.sql_keywords.contains(&"EXPLAIN".to_string()));
    assert!(s.extra.is_empty());
}

/// A settings.json as written by the Python app (model_dump_json), including
/// the path fields it stores.
const PYTHON_WRITTEN: &str = r##"{
  "default_data_var_name": "data",
  "default_limit": "500",
  "default_sql_font_size": "10",
  "default_result_font_size": "10",
  "default_sql_query": "SELECT * \nFROM $(default_data_var_name) \nLIMIT $(default_limit)",
  "default_sql_font": "Courier",
  "sql_keywords": ["IS", "LEFT JOIN", " where ", "IS"],
  "result_pagination_rows_per_page": "500",
  "save_file_history": "True",
  "max_rows": "1000",
  "current_theme": "Excel",
  "current_language": "ru",
  "enable_large_dataset_warning": true,
  "warning_criteria": "cells",
  "warn_on_exit_with_transforms": false,
  "copy_tuple_sample_size": 250,
  "warning_threshold_rows": 1000000,
  "warning_threshold_cells": 10000000,
  "warning_threshold_filesize_mb": 100,
  "bug_report_email": "x@example.com",
  "enable_crash_reporting": true,
  "colour_browseButton": "#8AFBF6",
  "colour_sqlEdit": "#D6EBAF",
  "colour_executeButton": "yellow",
  "colour_resultTable": "#9FF2F9",
  "colour_tableInfoButton": "#F3E5F5",
  "user_app_settings_dir": "/home/u/.ParVu",
  "usr_settings_file": "/home/u/.ParVu/settings/settings.json",
  "static_dir": "/usr/share/parvu/_internal/parvu/resources/static"
}"##;

#[test]
fn loads_python_written_file_and_round_trips_unknown_keys() {
    let s = Settings::from_json_lenient(PYTHON_WRITTEN).unwrap();
    assert_eq!(s.current_theme, "Excel");
    assert_eq!(s.current_language, "ru");
    assert_eq!(s.warning_criteria, WarningCriteria::Cells);
    assert!(!s.warn_on_exit_with_transforms);
    assert_eq!(s.copy_tuple_sample_size, 250);
    // model_post_init: upper-case, trimmed, de-duplicated.
    assert_eq!(s.sql_keywords, ["IS", "LEFT JOIN", "WHERE"]);
    assert_eq!(s.extra["static_dir"], json!("/usr/share/parvu/_internal/parvu/resources/static"));

    let out: serde_json::Value = serde_json::from_str(&s.to_json().unwrap()).unwrap();
    assert_eq!(out["default_limit"], json!("500"));
    assert_eq!(out["user_app_settings_dir"], json!("/home/u/.ParVu"));
    assert_eq!(out["colour_browseButton"], json!("#8AFBF6"));
    assert_eq!(Settings::from_json_lenient(&s.to_json().unwrap()).unwrap(), s);
}

#[test]
fn string_or_int_fields() {
    let s = Settings::from_json_lenient(
        r#"{"default_limit": 250, "max_rows": "  42 ", "default_sql_font_size": 13.0,
            "result_pagination_rows_per_page": "abc", "default_result_font_size": "-3"}"#,
    )
    .unwrap();
    assert_eq!(s.default_limit, IntOrString::Int(250));
    assert_eq!(s.default_limit(), 250);
    assert_eq!(s.max_rows(), 42);
    assert_eq!(s.default_sql_font_size, IntOrString::Int(13));
    // Unparsable or negative strings are kept as-is but accessors use defaults.
    assert_eq!(s.result_pagination_rows_per_page, IntOrString::Str("abc".into()));
    assert_eq!(s.rows_per_page(), 100);
    assert_eq!(s.default_result_font_size(), 10);
    // Ints serialise as ints, strings as strings.
    let v: serde_json::Value = serde_json::from_str(&s.to_json().unwrap()).unwrap();
    assert_eq!(v["default_limit"], json!(250));
    assert_eq!(v["max_rows"], json!("  42 "));
}

#[test]
fn wrong_typed_values_fall_back_per_field() {
    let s = Settings::from_json_lenient(
        r#"{"current_theme": "ParVu Black", "default_limit": [1], "enable_crash_reporting": "maybe",
            "warning_criteria": "bogus", "copy_tuple_sample_size": "x", "unknown_key": {"a": 1}}"#,
    )
    .unwrap();
    assert_eq!(s.current_theme, "ParVu Black");
    assert_eq!(s.default_limit, IntOrString::Int(1000));
    assert!(s.enable_crash_reporting);
    assert_eq!(s.warning_criteria, WarningCriteria::Rows);
    assert_eq!(s.copy_tuple_sample_size, 500);
    assert_eq!(s.extra["unknown_key"], json!({"a": 1}));
}

#[test]
fn save_file_history_flag() {
    for (v, on) in [("True", true), ("true", true), ("1", true), ("False", false), ("no", false)] {
        let s = Settings { save_file_history: v.into(), ..Settings::default() };
        assert_eq!(s.save_file_history_enabled(), on, "{v}");
    }
}

#[test]
fn first_run_with_missing_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(tmp.path().join("home").join(".ParVu"));
    let mgr = SettingsManager::with_paths(paths.clone());
    assert_eq!(mgr.settings().default_data_var_name, "data");
    assert_eq!(mgr.settings().default_limit(), 500); // bundled value
    assert_eq!(std::fs::read_to_string(paths.settings_file()).unwrap(), BUNDLED_SETTINGS_JSON);
    assert_eq!(
        std::fs::read_to_string(paths.default_settings_file()).unwrap(),
        BUNDLED_DEFAULT_SETTINGS_JSON
    );
    assert!(paths.recents_file().is_file());
}

#[test]
fn load_without_bundled_resources() {
    // Python test_load_without_bundled_resources: built-in defaults path.
    let tmp = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(tmp.path().join("home").join(".ParVu"));
    SettingsManager::reset_with(&paths, DefaultsSource::BuiltIn).unwrap();
    let loaded = SettingsManager::load(&paths);
    assert_eq!(loaded, Settings::default());
    assert!(paths.settings_file().is_file());
    assert!(paths.default_settings_file().is_file());
    assert_eq!(std::fs::read_to_string(paths.recents_file()).unwrap(), r#"{"recents": []}"#);
}

#[test]
fn corrupt_settings_file_is_reset() {
    let (_tmp, paths) = common::temp_app_dir();
    std::fs::create_dir_all(paths.settings_dir()).unwrap();
    std::fs::write(paths.settings_file(), "{ not json").unwrap();
    let s = SettingsManager::load(&paths);
    assert_eq!(s.default_limit(), 500);
    assert_eq!(std::fs::read_to_string(paths.settings_file()).unwrap(), BUNDLED_SETTINGS_JSON);

    // Non-object JSON is also treated as corrupt.
    std::fs::write(paths.settings_file(), "[1, 2]").unwrap();
    assert_eq!(SettingsManager::load(&paths).default_limit(), 500);
}

#[test]
fn existing_dir_without_settings_file() {
    let (_tmp, paths) = common::temp_app_dir();
    let s = SettingsManager::load(&paths);
    assert_eq!(s.default_limit(), 500);
    assert!(paths.settings_file().is_file());
}

#[test]
fn reset_keeps_valid_recents() {
    let (_tmp, paths) = common::temp_app_dir();
    std::fs::create_dir_all(paths.history_dir()).unwrap();
    std::fs::write(paths.recents_file(), r#"{"recents": ["/a.csv"]}"#).unwrap();
    SettingsManager::reset(&paths).unwrap();
    assert_eq!(std::fs::read_to_string(paths.recents_file()).unwrap(), r#"{"recents": ["/a.csv"]}"#);

    std::fs::write(paths.recents_file(), "garbage").unwrap();
    SettingsManager::reset(&paths).unwrap();
    assert_eq!(std::fs::read_to_string(paths.recents_file()).unwrap(), r#"{"recents":[]}"#);
}

#[test]
fn save_and_update() {
    let (_tmp, paths) = common::temp_app_dir();
    let mut mgr = SettingsManager::with_paths(paths.clone());
    let rejected = mgr
        .update(
            json!({"current_theme": "Excel", "default_limit": 42, "nope": 1, "max_rows": [1]})
                .as_object()
                .unwrap()
                .clone(),
        )
        .unwrap();
    assert_eq!(rejected, ["max_rows", "nope"]);
    assert_eq!(mgr.settings().current_theme, "Excel");
    assert_eq!(mgr.settings().max_rows(), 1000);

    let reloaded = SettingsManager::load(&paths);
    assert_eq!(reloaded.current_theme, "Excel");
    assert_eq!(reloaded.default_limit(), 42);

    mgr.settings_mut().current_language = "az".into();
    mgr.save().unwrap();
    assert_eq!(SettingsManager::with_paths(paths).settings().current_language, "az");
}
