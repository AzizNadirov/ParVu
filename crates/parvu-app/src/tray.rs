//! The tray icon (the top bar on GNOME, the notification area on Windows and
//! KDE) and what closing the last window does.
//!
//! * The icon's menu: "Open ParVu", a status line for AI clients, "Quit
//!   ParVu". A left click (Windows) opens the window too; the Linux
//!   appindicator only has the menu.
//! * With AI clients on and the `keep_running_in_tray` setting on, closing
//!   the last window hides it ([`close_action`]): its tabs, unsaved edits
//!   included, stay as they were, and AI clients keep working on them.
//! * Quitting from the tray (or `parvu --quit`) asks about unsaved edits
//!   in the windows that have them ([`quit_plan`]), as closing them would.
//!
//! On Linux the icon needs the appindicator library (`libayatana-appindicator3`,
//! loaded at run time) and a StatusNotifier host: on GNOME the AppIndicator
//! extension, which Ubuntu enables. Without a host, nothing hides into a
//! tray nobody can see: closing quits and `--background` starts minimised.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::state::{AppState, lock};

const OPEN: &str = "tray-open";
const STATUS: &str = "tray-status";
const QUIT: &str = "tray-quit";

/// The tray icon of the app, one per process (in [`AppState`]).
#[derive(Default)]
pub struct Tray {
    icon: Mutex<Option<(TrayIcon<Wry>, Items)>>,
    /// The icon exists (the host may still be missing; see [`available`]).
    built: AtomicBool,
    /// One menu update at a time, so the last one wins.
    updating: Mutex<()>,
}

struct Items {
    open: MenuItem<Wry>,
    status: MenuItem<Wry>,
    quit: MenuItem<Wry>,
}

/// The status line of the menu.
pub fn status_text(i18n: &parvu_config::I18n, enabled: bool, clients: usize) -> String {
    if enabled {
        i18n.tr("tray.status_on", &[("count", (clients as i64).into())])
    } else {
        i18n.t("tray.status_off")
    }
}

fn texts(app: &AppHandle) -> (String, String, String) {
    let state = app.state::<AppState>();
    let cfg = lock(&state.config);
    let enabled = cfg.settings.settings().mcp_allow_clients;
    let clients = crate::mcp_attach::client_count(app);
    (cfg.i18n.t("tray.open"), status_text(&cfg.i18n, enabled, clients), cfg.i18n.t("tray.quit"))
}

/// Create the tray icon. Logs and does nothing where there can be none.
pub fn start(app: &AppHandle) {
    if !library_loads() {
        tracing::warn!("tray: no appindicator library (libayatana-appindicator3-1); running without a tray icon");
        return;
    }
    let result = (|| -> tauri::Result<(TrayIcon<Wry>, Items)> {
        let (open_text, status_text, quit_text) = texts(app);
        let open = MenuItem::with_id(app, OPEN, open_text, true, None::<&str>)?;
        let status = MenuItem::with_id(app, STATUS, status_text, false, None::<&str>)?;
        let quit = MenuItem::with_id(app, QUIT, quit_text, true, None::<&str>)?;
        let sep = PredefinedMenuItem::separator(app)?;
        let menu = Menu::with_items(app, &[&open, &status, &sep, &quit])?;
        let mut builder = TrayIconBuilder::with_id("parvu")
            .icon(tauri::include_image!("icons/32x32.png"))
            .tooltip("ParVu")
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_menu_event(|app, event| match event.id().as_ref() {
                OPEN => show_app(app),
                QUIT => request_quit(app),
                _ => {}
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                    show_app(tray.app_handle());
                }
            });
        // Linux writes the icon to a file for the indicator; keep it with
        // ParVu's other runtime files rather than in a shared folder.
        if cfg!(target_os = "linux") {
            builder = builder.temp_dir_path(parvu_config::paths::user_cache_dir().join("tray"));
        }
        let icon = builder.build(app)?;
        Ok((icon, Items { open, status, quit }))
    })();
    match result {
        Ok(t) => {
            let tray = &app.state::<AppState>().tray;
            *lock(&tray.icon) = Some(t);
            tray.built.store(true, Ordering::SeqCst);
            tracing::info!("tray: icon created (host present: {})", host_present());
        }
        Err(e) => tracing::warn!("tray: could not create the icon: {e}"),
    }
}

/// Update the menu's texts (AI clients changed, or the language). A menu
/// change is handed to the main thread and waited for; it is made from a
/// thread of its own, so callers on the main thread (setup, before the event
/// loop runs) never wait for themselves.
pub fn refresh(app: &AppHandle) {
    if !app.state::<AppState>().tray.built.load(Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let _one = lock(&state.tray.updating);
        let Some((open, status, quit)) =
            lock(&state.tray.icon).as_ref().map(|(_, i)| (i.open.clone(), i.status.clone(), i.quit.clone()))
        else {
            return;
        };
        let (open_text, status_text, quit_text) = texts(&app);
        let _ = open.set_text(open_text);
        let _ = status.set_text(status_text);
        let _ = quit.set_text(quit_text);
    });
}

/// Whether the user can see the tray icon now, so a hidden window can be
/// found again. Asks the session bus on Linux (a few milliseconds).
pub fn available(app: &AppHandle) -> bool {
    app.state::<AppState>().tray.built.load(Ordering::SeqCst) && host_present()
}

#[cfg(target_os = "linux")]
fn library_loads() -> bool {
    // libappindicator-sys panics when it finds none of these; check first.
    ["libayatana-appindicator3.so.1", "libappindicator3.so.1", "libayatana-appindicator3.so", "libappindicator3.so"]
        .iter()
        .any(|name| unsafe { libloading::Library::new(name) }.is_ok())
}

#[cfg(not(target_os = "linux"))]
fn library_loads() -> bool {
    true
}

/// A StatusNotifier host is registered: something draws tray icons.
#[cfg(target_os = "linux")]
fn host_present() -> bool {
    use gio::glib::{ToVariant, Variant, VariantTy};
    let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>) else { return false };
    let call = |iface: &str, method: &str, args: Variant, reply: &str| {
        bus.call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            iface,
            method,
            Some(&args),
            VariantTy::new(reply).ok(),
            gio::DBusCallFlags::NONE,
            1000,
            None::<&gio::Cancellable>,
        )
    };
    let watcher = "org.kde.StatusNotifierWatcher";
    let owned = call("org.freedesktop.DBus", "NameHasOwner", (watcher,).to_variant(), "(b)")
        .ok()
        .and_then(|v| v.get::<(bool,)>())
        .is_some_and(|(b,)| b);
    if !owned {
        return false;
    }
    // A watcher without a host draws nothing; when the property can't be
    // read, trust the watcher.
    let host = bus.call_sync(
        Some(watcher),
        "/StatusNotifierWatcher",
        "org.freedesktop.DBus.Properties",
        "Get",
        Some(&(watcher, "IsStatusNotifierHostRegistered").to_variant()),
        VariantTy::new("(v)").ok(),
        gio::DBusCallFlags::NONE,
        1000,
        None::<&gio::Cancellable>,
    );
    match host {
        Ok(v) => v.get::<(Variant,)>().and_then(|(inner,)| inner.get::<bool>()).unwrap_or(true),
        Err(_) => true,
    }
}

#[cfg(not(target_os = "linux"))]
fn host_present() -> bool {
    true
}

// ------------------------------------------------------------ the decisions

/// What closing a window does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseAction {
    /// Hide it; ParVu keeps running in the tray with its tabs as they are.
    Hide,
    /// Close it right away: nothing would be lost.
    Close,
    /// Ask about unsaved edits (and applied steps, if the user wants that
    /// warning) first, as closing always did.
    Confirm,
}

/// Everything [`close_action`] looks at.
#[derive(Debug, Clone, Copy, Default)]
pub struct CloseContext {
    /// The `keep_running_in_tray` setting.
    pub keep_in_tray: bool,
    /// The `mcp_allow_clients` setting.
    pub ai_clients: bool,
    /// The tray icon is shown (see [`available`]).
    pub tray: bool,
    /// This is the only window.
    pub last_window: bool,
    /// Closing would lose something: unsaved edits, or applied steps when
    /// the "warn when closing with applied transforms" setting is on.
    pub dirty: bool,
}

/// Decide what closing a window does. Hiding needs all of: AI clients on
/// (that is what keeping ParVu running is for), the setting on, an icon the
/// user can see (otherwise the hidden window could never come back) and the
/// last window (other windows close as always). A hidden window loses
/// nothing, so it doesn't ask about unsaved edits; Quit asks later.
pub fn close_action(c: CloseContext) -> CloseAction {
    if c.keep_in_tray && c.ai_clients && c.tray && c.last_window {
        CloseAction::Hide
    } else if c.dirty {
        CloseAction::Confirm
    } else {
        CloseAction::Close
    }
}

/// What "Quit ParVu" does with the open windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuitPlan {
    /// Close these now: they have nothing to lose.
    pub close: Vec<String>,
    /// Show these and ask, as closing them would. When the user agrees for
    /// the last one, ParVu ends.
    pub ask: Vec<String>,
}

impl QuitPlan {
    /// Quit at once.
    pub fn exit_now(&self) -> bool {
        self.ask.is_empty()
    }
}

/// Plan a quit from `(window label, dirty)` pairs.
pub fn quit_plan(windows: &[(String, bool)]) -> QuitPlan {
    let (ask, close): (Vec<_>, Vec<_>) = windows.iter().partition(|(_, dirty)| *dirty);
    QuitPlan { close: close.into_iter().map(|(l, _)| l.clone()).collect(), ask: ask.into_iter().map(|(l, _)| l.clone()).collect() }
}

/// Whether closing a window would lose something, from its tabs'
/// `(unsaved, applied steps)`: unsaved are cell edits, sheet changes or a
/// table operation's result not saved to a file. The UI asks about the same
/// (`windowIsDirty` in `ui/src/dialogs/logic.ts`).
pub fn dirty(tabs: impl IntoIterator<Item = (bool, usize)>, warn_on_transforms: bool) -> bool {
    tabs.into_iter().any(|(unsaved, steps)| unsaved || (warn_on_transforms && steps > 0))
}

// ------------------------------------------------------------- the actions

/// "Open ParVu": bring the window up (the one used last), or open one.
pub fn show_app(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.background.store(false, Ordering::SeqCst);
    match crate::pick_window(app) {
        Some(w) => crate::bring_forward(&w),
        None => {
            let n = state.next_window.fetch_add(1, Ordering::Relaxed);
            if let Err(e) = crate::build_window(app, &format!("window-{n}")) {
                tracing::error!("tray: could not open a window: {e}");
            }
        }
    }
}

/// "Quit ParVu": close the windows with nothing to lose, show the others
/// and let their UI ask (`quit-requested`); ParVu ends with the last one.
pub fn request_quit(app: &AppHandle) {
    let app = app.clone();
    // Workspaces are locked to look at their tabs: not on the main thread.
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let warn = lock(&state.config).settings.settings().warn_on_exit_with_transforms;
        let mut windows: Vec<(String, bool)> = app
            .webview_windows()
            .keys()
            .map(|label| {
                let ws = lock(&state.workspaces).get(label).cloned();
                let dirty = ws.is_some_and(|ws| {
                    let tabs = lock(&ws).summaries();
                    let unsaved = |t: &parvu_core::TabSummary| t.pending_edits > 0 || t.sheets_changed || t.origin.is_some();
                    dirty(tabs.iter().map(|t| (unsaved(t), t.steps.len())), warn)
                });
                (label.clone(), dirty)
            })
            .collect();
        windows.sort();
        let plan = quit_plan(&windows);
        tracing::info!("quit: closing {:?}, asking in {:?}", plan.close, plan.ask);
        if plan.exit_now() {
            app.exit(0);
            return;
        }
        for label in &plan.close {
            if let Some(w) = app.get_webview_window(label) {
                let _ = w.destroy();
            }
        }
        state.background.store(false, Ordering::SeqCst);
        for label in &plan.ask {
            if let Some(w) = app.get_webview_window(label) {
                crate::bring_forward(&w);
                let _ = app.emit_to(label.as_str(), "quit-requested", ());
            }
        }
    });
}

// ---------------------------------------------------------------- commands

/// What the UI should do when the user closes its window.
#[derive(Debug, Serialize)]
pub struct CloseDecision {
    pub action: CloseAction,
    /// First time ParVu hides into the tray: tell the user where it went.
    pub notice: bool,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn window_close_action(app: AppHandle, window: tauri::WebviewWindow, dirty: bool) -> CloseDecision {
    let state = app.state::<AppState>();
    let (keep_in_tray, ai_clients, noticed) = {
        let cfg = lock(&state.config);
        let s = cfg.settings.settings();
        (s.keep_running_in_tray, s.mcp_allow_clients, s.tray_notice_shown)
    };
    let last_window = app.webview_windows().keys().all(|l| l == window.label());
    let tray = available(&app);
    let action = close_action(CloseContext { keep_in_tray, ai_clients, tray, last_window, dirty });
    CloseDecision { action, notice: action == CloseAction::Hide && !noticed }
}

/// Hide the calling window into the tray.
#[tauri::command(rename_all = "snake_case")]
pub fn hide_to_tray(app: AppHandle, window: tauri::WebviewWindow) -> crate::error::ApiResult<()> {
    let state = app.state::<AppState>();
    {
        let mut cfg = lock(&state.config);
        if !cfg.settings.settings().tray_notice_shown {
            let mut values = serde_json::Map::new();
            values.insert("tray_notice_shown".into(), true.into());
            cfg.settings.update(values)?;
        }
    }
    // The user chose to keep it running: no idle quit from now on.
    let untouched = state.background.load(Ordering::SeqCst);
    state.background.store(crate::instance::untouched_after(untouched, crate::instance::Touch::ClosedToTray), Ordering::SeqCst);
    tracing::info!("window {} hidden into the tray", window.label());
    window.hide()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_hides_only_with_tray_ai_setting_and_last_window() {
        for keep_in_tray in [false, true] {
            for ai_clients in [false, true] {
                for tray in [false, true] {
                    for last_window in [false, true] {
                        for dirty in [false, true] {
                            let c = CloseContext { keep_in_tray, ai_clients, tray, last_window, dirty };
                            let expected = if keep_in_tray && ai_clients && tray && last_window {
                                // Hidden with its tabs and edits; Quit asks later.
                                CloseAction::Hide
                            } else if dirty {
                                CloseAction::Confirm
                            } else {
                                CloseAction::Close
                            };
                            assert_eq!(close_action(c), expected, "{c:?}");
                        }
                    }
                }
            }
        }
        // The everyday cases, spelled out.
        let on = CloseContext { keep_in_tray: true, ai_clients: true, tray: true, last_window: true, dirty: true };
        assert_eq!(close_action(on), CloseAction::Hide);
        assert_eq!(close_action(CloseContext { ai_clients: false, ..on }), CloseAction::Confirm);
        assert_eq!(close_action(CloseContext { keep_in_tray: false, dirty: false, ..on }), CloseAction::Close);
        assert_eq!(close_action(CloseContext { tray: false, ..on }), CloseAction::Confirm);
    }

    #[test]
    fn quit_asks_only_where_something_would_be_lost() {
        let w = |l: &str, d: bool| (l.to_string(), d);
        let plan = quit_plan(&[w("main", false), w("window-2", false)]);
        assert!(plan.exit_now());
        assert_eq!(plan.close, ["main", "window-2"]);
        let plan = quit_plan(&[w("main", true), w("window-2", false), w("window-3", true)]);
        assert!(!plan.exit_now());
        assert_eq!(plan.close, ["window-2"]);
        assert_eq!(plan.ask, ["main", "window-3"]);
        assert!(quit_plan(&[]).exit_now());
    }

    #[test]
    fn dirty_counts_edits_and_steps_when_warned() {
        assert!(!dirty([], true));
        assert!(!dirty([(false, 0)], true));
        assert!(dirty([(false, 0), (true, 0)], false));
        assert!(dirty([(false, 1)], true));
        assert!(!dirty([(false, 1)], false));
    }

    #[test]
    fn status_line_in_every_language() {
        for lang in ["en", "ru", "az"] {
            let i18n = parvu_config::I18n::with_language(lang);
            let on = status_text(&i18n, true, 2);
            assert!(on.contains('2') && !on.contains("{count}"), "{lang}: {on}");
            assert_ne!(status_text(&i18n, false, 0), "tray.status_off");
            for key in ["tray.open", "tray.quit"] {
                assert_ne!(i18n.t(key), key, "{lang}");
            }
        }
    }
}
