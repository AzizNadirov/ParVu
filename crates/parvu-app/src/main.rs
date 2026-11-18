// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai_assistant;
mod bridge;
mod column_types;
mod commands;
mod custom;
mod custom_store;
mod error;
#[cfg(test)]
mod excel_tests;
mod instance;
mod jobs;
mod mcp;
mod mcp_attach;
mod memguard;
mod state;

use std::path::PathBuf;

use std::sync::atomic::Ordering;

use parvu_config::{I18n, RecentsManager, SettingsManager, ThemeManager};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::state::{AppState, Config};

const CRASH_MARKER: &str = "last_crash.txt";

/// When this process started. Focus events in the first seconds of a
/// background start come from the window appearing, not from the user.
static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
const SETTLE: std::time::Duration = std::time::Duration::from_secs(3);

/// Create a top-level window. Every window loads the same UI and gets its own
/// set of tabs.
pub fn build_window(app: &AppHandle, label: &str) -> tauri::Result<()> {
    window_builder(app, label).build()?;
    Ok(())
}

fn window_builder<'a>(app: &'a AppHandle, label: &str) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title("ParVu")
        .inner_size(1400.0, 900.0)
        .min_inner_size(900.0, 600.0)
}

/// The first window of a `--background` start: minimised, without taking
/// the focus from what the user is doing.
fn build_background_window(app: &AppHandle) -> tauri::Result<()> {
    let w = window_builder(app, "main").visible(false).focused(false).build()?;
    if cfg!(target_os = "linux") {
        // On X11, GTK iconifies a window that isn't shown yet, so it appears
        // minimised without flashing on screen. Wayland has no way to start
        // minimised: GTK ignores this until the window is on screen, so
        // minimise again right after it appears.
        w.minimize()?;
        w.show()?;
        let (app, w) = (app.clone(), w.clone());
        std::thread::spawn(move || {
            for ms in [200, 1000] {
                std::thread::sleep(std::time::Duration::from_millis(ms));
                if app.state::<AppState>().background.load(Ordering::SeqCst) {
                    let _ = w.minimize();
                }
            }
        });
    } else {
        // Windows and macOS keep a hidden window hidden when minimising it;
        // show it without focus, then minimise.
        w.show()?;
        w.minimize()?;
    }
    Ok(())
}

/// Bring `w` to the front (restored if minimised).
fn bring_forward(w: &WebviewWindow) {
    let _ = w.unminimize();
    let _ = w.show();
    let _ = w.set_focus();
}

/// Another `parvu` start handed us its files (or just wants the window).
fn handle_second_start(app: &AppHandle, req: instance::Request) {
    let state = app.state::<AppState>();
    if req.show || !req.files.is_empty() {
        state.background.store(false, Ordering::SeqCst);
    }
    let label = {
        let active = state.attach.active_window();
        let windows = app.webview_windows();
        if windows.contains_key(&active) { active } else { windows.keys().min().cloned().unwrap_or_else(|| "main".into()) }
    };
    if !req.files.is_empty() {
        // Before the first window's UI asked for its start files, add them
        // there; after, the UI opens them on this event.
        let mut pending = crate::state::lock(&state.initial_files);
        if state.booted.load(Ordering::SeqCst) {
            let _ = app.emit_to(label.as_str(), "open-files", &req.files);
        } else {
            pending.extend(req.files.iter().cloned());
        }
    }
    if req.show
        && let Some(w) = app.get_webview_window(&label)
    {
        bring_forward(&w);
    }
}

/// Quit a background ParVu nobody opened once AI clients are gone a while.
fn start_idle_quit(app: &AppHandle) {
    let app = app.clone();
    let grace = instance::idle_grace();
    let step = (grace / 4).clamp(std::time::Duration::from_millis(250), std::time::Duration::from_secs(5));
    let _ = std::thread::Builder::new().name("parvu-idle".into()).spawn(move || {
        let mut clock = instance::IdleClock::new(grace);
        loop {
            std::thread::sleep(step);
            let state = app.state::<AppState>();
            let untouched = state.background.load(Ordering::SeqCst);
            if !untouched {
                return;
            }
            if clock.tick(std::time::Instant::now(), untouched, mcp_attach::client_count(&app)) {
                tracing::info!("background start: no AI client for {grace:?} and the window was never opened; quitting");
                app.exit(0);
                return;
            }
        }
    });
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
    STARTED.get_or_init(std::time::Instant::now);
    let _log_guard = init_logging();
    install_crash_hook();
    // `parvu mcp …`: an MCP server on stdin/stdout, without a window.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("mcp") {
        std::process::exit(mcp::run(args[1..].to_vec()));
    }
    let args = instance::AppArgs::parse(std::env::args_os().skip(1));
    let initial_files: Vec<PathBuf> =
        args.files.iter().filter(|p| p.is_file()).map(|p| std::path::absolute(p).unwrap_or_else(|_| p.clone())).collect();
    // One ParVu per profile: a second start hands its files to the first.
    let request = instance::Request { files: initial_files.clone(), show: !args.background };
    let _instance_lock = match instance::claim(&request) {
        instance::Startup::Primary(lock) => lock,
        instance::Startup::Forwarded => {
            tracing::info!("ParVu is already running; passed the start on to it");
            return;
        }
    };
    let sole = _instance_lock.is_some();
    let crash = take_crash_report();
    let config = load_config();
    // A background start exists to serve AI clients. With them switched
    // off it would serve nothing, so it opens as a normal window instead.
    let background = args.background && config.settings.settings().mcp_allow_clients;
    tracing::info!("ParVu {} starting{}", env!("CARGO_PKG_VERSION"), if background { " in the background" } else { "" });
    parvu_ai::init_tls();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new(config, initial_files, crash))
        .setup(move |app| {
            let state = app.state::<AppState>();
            state.background.store(background, Ordering::SeqCst);
            if background {
                build_background_window(app.handle())?;
                start_idle_quit(app.handle());
            } else {
                build_window(app.handle(), "main")?;
            }
            if sole {
                // No other ParVu of this profile can be listening.
                mcp::clear_stale_run_file();
            }
            {
                let app = app.handle().clone();
                if let Err(e) = instance::serve(move |req| handle_second_start(&app, req)) {
                    tracing::error!("single instance: {e}");
                }
            }
            state.attach.set_active_window("main");
            custom::start(app.handle());
            {
                let limit_app = app.handle().clone();
                let notify_app = app.handle().clone();
                memguard::start(
                    move || {
                        let state = limit_app.state::<AppState>();
                        let mb = state.config.lock().map(|c| c.settings.settings().query_memory_limit_mb).unwrap_or(1024);
                        std::env::var("PARVU_MEMORY_LIMIT_MB")
                            .ok()
                            .and_then(|v| v.trim().parse().ok())
                            .unwrap_or(mb.max(0) as u64)
                    },
                    move |mb| {
                        let _ = notify_app.emit("memory-guard", mb);
                    },
                );
            }
            let allow_clients = state.config.lock().map(|c| c.settings.settings().mcp_allow_clients).unwrap_or(false);
            if allow_clients && let Err(e) = mcp_attach::start(app.handle()) {
                tracing::error!("AI clients: {e}");
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            let Some(state) = window.app_handle().try_state::<AppState>() else { return };
            match event {
                tauri::WindowEvent::Destroyed => {
                    state.jobs.cancel_window(window.label());
                    state.drop_workspace(window.label())
                }
                tauri::WindowEvent::Focused(true) => {
                    state.attach.set_active_window(window.label());
                    // The user brought a background window up: from now on
                    // it is a normal ParVu that stays until they close it.
                    let settled = STARTED.get().is_some_and(|t| t.elapsed() > SETTLE);
                    if settled && !window.is_minimized().unwrap_or(true) && window.is_visible().unwrap_or(false) {
                        state.background.store(false, Ordering::SeqCst);
                    }
                }
                _ => {}
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
            commands::make_table,
            commands::value_suggestions,
            commands::refresh_table,
            commands::table_to_file,
            commands::excel_sheets,
            commands::sheet_op,
            commands::set_sheet,
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
            commands::source_columns,
            commands::preview_csv,
            commands::set_source_reading,
            commands::preview_conversion,
            commands::suggest_reading,
            commands::preview_column_fn,
            commands::set_formula,
            commands::remove_formula,
            commands::json_keys,
            commands::conversion_error_count,
            jobs::cancel_job,
            commands::add_computed_column,
            commands::replace_values,
            commands::undo,
            commands::reset_tab,
            commands::edit_cell,
            commands::edit_cells,
            commands::column_ranges,
            commands::chart_data,
            commands::save_chart_image,
            commands::save_tab,
            commands::column_stats,
            commands::unique_values,
            commands::column_tuple,
            commands::find,
            commands::start_export,
            commands::cancel_export,
            mcp_attach::mcp_status,
            mcp_attach::mcp_set_enabled,
            mcp_attach::mcp_decide,
            ai_assistant::ai_providers,
            ai_assistant::ai_save_provider,
            ai_assistant::ai_delete_provider,
            ai_assistant::ai_test_provider,
            ai_assistant::ai_ask,
            ai_assistant::ai_cancel,
            ai_assistant::ai_clear_chat,
            custom::custom_list,
            custom::custom_validate,
            custom::custom_save,
            custom::custom_delete,
            custom::custom_restore_samples,
            custom::custom_export,
            custom::custom_import_preview,
            custom::custom_import,
            custom::custom_value,
            custom::custom_preview,
            custom::custom_from_steps,
        ])
        .build(tauri::generate_context!())
        .expect("error while running ParVu")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                mcp_attach::stop(app);
                instance::clear_control_file();
            }
        });
}
