// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod error;
mod state;

use std::path::PathBuf;

use parvu_config::{I18n, RecentsManager, SettingsManager, ThemeManager};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::state::{AppState, Config};

const CRASH_MARKER: &str = "last_crash.txt";

/// Create a top-level window. Every window loads the same UI and gets its own
/// set of tabs.
pub fn build_window(app: &AppHandle, label: &str) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title("ParVu")
        .inner_size(1400.0, 900.0)
        .min_inner_size(900.0, 600.0)
        .build()?;
    Ok(())
}

fn init_logging() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::{EnvFilter, fmt};
    let dir = parvu_config::paths::log_dir().ok()?;
    let appender = tracing_appender::rolling::daily(&dir, "parvu.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_from_env("PARVU_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).with_writer(writer).with_ansi(false).init();
    Some(guard)
}

/// Write a crash report on panic; the next start shows it to the user.
fn install_crash_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = format!(
            "ParVu {} crashed at {}\nOS: {} {}\n\n{info}\n\nBacktrace:\n{}",
            env!("CARGO_PKG_VERSION"),
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::backtrace::Backtrace::force_capture()
        );
        tracing::error!("{report}");
        if let Ok(dir) = parvu_config::paths::log_dir() {
            let _ = std::fs::write(dir.join(CRASH_MARKER), &report);
        }
        default(info);
    }));
}

fn take_crash_report() -> Option<String> {
    let path = parvu_config::paths::log_dir().ok()?.join(CRASH_MARKER);
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    Some(text)
}

fn load_config() -> Config {
    let settings = SettingsManager::new();
    let recents = RecentsManager::new();
    let mut themes = ThemeManager::new();
    themes.apply_settings(settings.settings());
    let i18n = I18n::with_language(&settings.settings().current_language);
    Config { settings, recents, themes, i18n }
}

fn main() {
    let _log_guard = init_logging();
    install_crash_hook();
    let crash = take_crash_report();
    let initial_files: Vec<PathBuf> =
        std::env::args_os().skip(1).map(PathBuf::from).filter(|p| p.is_file()).collect();
    tracing::info!("ParVu {} starting", env!("CARGO_PKG_VERSION"));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new(load_config(), initial_files, crash))
        .setup(|app| {
            build_window(app.handle(), "main")?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event
                && let Some(state) = window.app_handle().try_state::<AppState>()
            {
                state.drop_workspace(window.label());
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::update_settings,
            commands::reset_settings,
            commands::set_language,
            commands::set_theme,
            commands::theme_css,
            commands::list_themes,
            commands::import_theme,
            commands::export_theme,
            commands::delete_theme,
            commands::recents,
            commands::clear_recents,
            commands::new_window,
            commands::open_file,
            commands::close_tab,
            commands::tabs,
            commands::tab_summary,
            commands::get_page,
            commands::columns,
            commands::run_sql,
            commands::catalog,
            commands::run_expression,
            commands::preview_expression,
            commands::apply_op,
            commands::add_computed_column,
            commands::replace_values,
            commands::undo,
            commands::reset_tab,
            commands::edit_cell,
            commands::save_tab,
            commands::column_stats,
            commands::unique_values,
            commands::column_tuple,
            commands::find,
            commands::start_export,
            commands::cancel_export,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ParVu");
}
