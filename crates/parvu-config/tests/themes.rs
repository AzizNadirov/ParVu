mod common;

use parvu_config::themes::{DEFAULT_THEME_NAME, builtin_themes};
use parvu_config::{ColorScheme, ConfigError, Settings, Theme, ThemeManager, theme_css_variables};

#[test]
fn theme_manager_builtin() {
    let (_tmp, mgr) = common::theme_manager();
    let themes = mgr.list_themes();
    for name in ["ParVu Light", "Excel", "ParVu Black"] {
        assert!(themes.contains(&name.to_string()), "{name}");
    }
    // Built-ins are written to the themes folder for reference.
    for f in ["parvu_light.json", "excel.json", "parvu_black.json"] {
        assert!(mgr.themes_dir().join(f).is_file(), "{f}");
    }
    assert_eq!(themes.len(), 3, "built-in files must not be listed twice");
}

#[test]
fn theme_manager_set_theme() {
    let (_tmp, mut mgr) = common::theme_manager();
    assert!(mgr.set_theme("ParVu Light"));
    assert_eq!(mgr.current_theme().name, "ParVu Light");
    assert!(mgr.set_theme("ParVu Black"));
    assert_eq!(mgr.current_theme().colors.background, "#1E1E1E");
}

#[test]
fn theme_manager_invalid_theme() {
    let (_tmp, mut mgr) = common::theme_manager();
    assert!(!mgr.set_theme("Nonexistent Theme"));
    assert_eq!(mgr.current_theme().name, DEFAULT_THEME_NAME);
}

#[test]
fn apply_settings_with_fallback() {
    let (_tmp, mut mgr) = common::theme_manager();
    let s = Settings { current_theme: "Excel".into(), ..Settings::default() };
    assert_eq!(mgr.apply_settings(&s).name, "Excel");
    let s = Settings { current_theme: "Gone".into(), ..Settings::default() };
    assert_eq!(mgr.apply_settings(&s).name, "ParVu Light");
}

#[test]
fn theme_to_json() {
    let theme = Theme { colors: ColorScheme { background: "#FFFFFF".into(), ..ColorScheme::default() }, ..Theme::new("Test") };
    let json = theme.to_json().unwrap();
    assert!(json.contains("Test"));
    assert!(json.contains("#FFFFFF"));
}

#[test]
fn builtin_themes_round_trip_and_css() {
    let themes = builtin_themes();
    assert_eq!(themes.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["ParVu Light", "Excel", "ParVu Black"]);
    for theme in themes {
        let back = Theme::from_json(&theme.to_json().unwrap()).unwrap();
        assert_eq!(back, theme);

        let css = theme_css_variables(&theme);
        assert!(css.starts_with(":root {\n") && css.ends_with("}\n"));
        let colors = serde_json::to_value(&theme.colors).unwrap();
        for (k, v) in colors.as_object().unwrap() {
            let line = format!("--pv-colors-{}: {};", k.replace('_', "-"), v.as_str().unwrap());
            assert!(css.contains(&line), "{}: missing {line}", theme.name);
        }
        let layout = serde_json::to_value(&theme.layout).unwrap();
        for k in layout.as_object().unwrap().keys() {
            assert!(css.contains(&format!("--pv-layout-{}: ", k.replace('_', "-"))), "{}: {k}", theme.name);
        }
        // colours + layout + 2 derived
        let n = css.lines().filter(|l| l.trim_start().starts_with("--pv-")).count();
        assert_eq!(n, colors.as_object().unwrap().len() + layout.as_object().unwrap().len() + 2);
    }
}

#[test]
fn css_value_formats() {
    let mut t = Theme::new("T");
    t.layout.show_grid = false;
    t.layout.alternate_row_colors = false;
    t.layout.code_font_family = "Fira \"Code\"; }".into();
    t.colors.table_alternate_row = "#123456".into();
    let css = theme_css_variables(&t);
    assert!(css.contains("  --pv-colors-table-header-background: #E0E0E0;\n"));
    assert!(css.contains("  --pv-layout-window-min-width: 1200px;\n"));
    assert!(css.contains("  --pv-layout-code-font-size: 10pt;\n"));
    assert!(css.contains("  --pv-layout-default-font-family: \"Arial\";\n"));
    assert!(css.contains("  --pv-layout-code-font-family: \"Fira \\\"Code\\\" \";\n"));
    assert!(css.contains("  --pv-layout-show-grid: 0;\n"));
    assert!(css.contains("  --pv-layout-show-reset-button: 1;\n"));
    assert!(css.contains("  --pv-derived-table-grid: transparent;\n"));
    assert!(css.contains("  --pv-derived-table-alternate-row: #FFFFFF;\n"));
}

const PYTHON_PARTIAL_THEME: &str = r##"{
  "name": "My Custom",
  "colors": {"background": "#101010", "foreground": "#EEEEEE", "some_future_color": "#000"},
  "layout": {"code_font_size": 14},
  "extra_top_level": true
}"##;

#[test]
fn custom_themes_load_from_folder() {
    let (_tmp, mut mgr) = common::theme_manager();
    std::fs::write(mgr.themes_dir().join("my_custom.json"), PYTHON_PARTIAL_THEME).unwrap();
    std::fs::write(mgr.themes_dir().join("broken.json"), "nope").unwrap();
    std::fs::write(mgr.themes_dir().join("readme.txt"), "x").unwrap();

    assert_eq!(mgr.list_themes(), ["Excel", "My Custom", "ParVu Black", "ParVu Light"]);
    let t = mgr.get_theme("My Custom").unwrap();
    assert_eq!(t.colors.background, "#101010");
    assert_eq!(t.colors.menu_hover, ColorScheme::default().menu_hover);
    assert_eq!(t.layout.code_font_size, 14);
    assert_eq!(t.author, "ParVu");
    assert_eq!(t.version, "1.0");
    assert!(mgr.set_theme("My Custom"));
    assert!(mgr.css_variables().contains("--pv-colors-background: #101010;"));
    assert!(mgr.get_theme("Broken").is_none());
}

#[test]
fn import_export_delete() {
    let (tmp, mgr) = common::theme_manager();
    let src = tmp.path().join("incoming.json");
    std::fs::write(&src, PYTHON_PARTIAL_THEME).unwrap();
    assert_eq!(mgr.import_theme(&src).unwrap(), "My Custom");
    assert!(mgr.themes_dir().join("my_custom.json").is_file());

    let out = tmp.path().join("out.json");
    mgr.export_theme("ParVu Black", &out).unwrap();
    assert_eq!(Theme::load_from_file(&out).unwrap().name, "ParVu Black");
    assert!(matches!(mgr.export_theme("Nope", &out), Err(ConfigError::ThemeNotFound(_))));

    assert!(matches!(mgr.delete_theme("Excel"), Err(ConfigError::BuiltinTheme(_))));
    mgr.delete_theme("My Custom").unwrap();
    assert!(mgr.get_theme("My Custom").is_none());
    assert!(matches!(mgr.delete_theme("My Custom"), Err(ConfigError::ThemeNotFound(_))));
}

#[test]
fn existing_builtin_files_are_not_overwritten() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("themes");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("excel.json"), r#"{"name": "Excel", "description": "user edited"}"#).unwrap();
    let mgr = ThemeManager::with_themes_dir(&dir);
    assert!(std::fs::read_to_string(dir.join("excel.json")).unwrap().contains("user edited"));
    // Built-in definition still wins for the name, as in Python.
    assert_ne!(mgr.get_theme("Excel").unwrap().description, "user edited");
}
