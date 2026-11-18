//! Attached MCP mode inside the app (plans/AI_PLAN.md §19.1): AI clients
//! such as Claude Code or Claude Desktop connect through
//! `parvu mcp --attach` and work on the focused window's tabs. Their changes
//! appear as approval cards (`mcp-proposal` events); the user's answer comes
//! back through `mcp_decide`.
//!
//! The listener runs only while the `mcp_allow_clients` setting is on. It
//! binds 127.0.0.1 on a random port and accepts only connections that send
//! the random token from `~/.ParVu/run/mcp.json`, which only the user can read.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use parvu_core::Workspace;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;

use crate::error::{ApiError, ApiResult};
use crate::mcp::{self, AttachHost, CustomOffer, Decision, ParvuMcp, Proposal, RunInfo};
use crate::state::{AppState, lock};

/// Attached-mode state, one per app (in [`AppState`]).
#[derive(Default)]
pub struct Attach {
    running: Mutex<Option<Running>>,
    clients: AtomicUsize,
    pending: Mutex<HashMap<u64, oneshot::Sender<bool>>>,
    next_id: AtomicU64,
    active_window: Mutex<String>,
    last_error: Mutex<Option<String>>,
}

struct Running {
    task: tauri::async_runtime::JoinHandle<()>,
    port: u16,
}

impl Attach {
    /// The window AI clients work on: the one focused last.
    pub fn set_active_window(&self, label: &str) {
        *lock(&self.active_window) = label.to_string();
    }

    pub fn active_window(&self) -> String {
        lock(&self.active_window).clone()
    }
}

/// How many AI clients are connected now.
pub fn client_count(app: &AppHandle) -> usize {
    app.state::<AppState>().attach.clients.load(Ordering::SeqCst)
}

/// Make sure the user sees a request waiting in `window`: a window hidden
/// in the tray is shown minimised first (a card in a hidden window would
/// never be seen); when the window is minimised or in the background, ask
/// the system to draw attention to it (the taskbar entry flashes or is
/// marked urgent) without moving it in front of the user's work.
fn call_attention(app: &AppHandle, window: &str) {
    let Some(w) = app.get_webview_window(window) else { return };
    if !w.is_visible().unwrap_or(true) {
        tracing::info!("MCP: showing window {window}, hidden in the tray, for a request");
        if let Err(e) = crate::show_minimised(&w) {
            tracing::warn!("MCP: could not show the window: {e}");
        }
        let state = app.state::<AppState>();
        let untouched = state.background.load(Ordering::SeqCst);
        state.background.store(
            crate::instance::untouched_after(untouched, crate::instance::Touch::ShownForApproval),
            Ordering::SeqCst,
        );
    }
    let minimized = w.is_minimized().unwrap_or(false);
    let focused = w.is_focused().unwrap_or(true);
    if minimized || !focused {
        tracing::info!("MCP: asking for attention to window {window} (minimised: {minimized}, focused: {focused})");
        if let Err(e) = w.request_user_attention(Some(tauri::UserAttentionType::Critical)) {
            tracing::warn!("MCP: could not request attention: {e}");
        }
    }
}

/// What the "Connect an AI client" dialog shows.
#[derive(Debug, Serialize)]
pub struct McpStatus {
    pub enabled: bool,
    pub listening: bool,
    pub port: Option<u16>,
    pub clients: usize,
    /// How clients start the bridge: the program and its arguments.
    pub command: String,
    pub args: Vec<String>,
    pub error: Option<String>,
}

/// The program name clients should run: just `parvu` when it is on PATH,
/// the full path otherwise (development builds, portable installs).
fn bridge_command() -> String {
    let Ok(exe) = std::env::current_exe() else { return "parvu".into() };
    let on_path = exe.parent().is_some_and(|dir| {
        std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d == dir))
    });
    match (on_path, exe.file_stem()) {
        (true, Some(name)) => name.to_string_lossy().into_owned(),
        _ => exe.display().to_string(),
    }
}

pub fn status(app: &AppHandle) -> McpStatus {
    let state = app.state::<AppState>();
    let at = &state.attach;
    let enabled = lock(&state.config).settings.settings().mcp_allow_clients;
    let port = lock(&at.running).as_ref().map(|r| r.port);
    McpStatus {
        enabled,
        listening: port.is_some(),
        port,
        clients: at.clients.load(Ordering::SeqCst),
        command: bridge_command(),
        args: vec!["mcp".into(), "--attach".into()],
        error: lock(&at.last_error).clone(),
    }
}

/// Start the listener (no-op when it runs already).
pub fn start(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let at = &state.attach;
    let mut running = lock(&at.running);
    if running.is_some() {
        return Ok(());
    }
    let result: Result<Running, String> = (|| {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let token = mcp::new_token();
        let info = RunInfo { port, token: token.clone(), pid: std::process::id() };
        mcp::write_run_file(&mcp::run_file(), &info).map_err(|e| format!("{}: {e}", mcp::run_file().display()))?;
        let (make_app, count_app) = (app.clone(), app.clone());
        let task = tauri::async_runtime::spawn(async move {
            let listener = match tokio::net::TcpListener::from_std(listener) {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!("MCP listener: {e}");
                    return;
                }
            };
            mcp::serve_tcp(
                listener,
                token,
                move || ParvuMcp::attached(Arc::new(TauriHost { app: make_app.clone(), window: None })),
                move |n| {
                    count_app.state::<AppState>().attach.clients.store(n, Ordering::SeqCst);
                    let _ = count_app.emit("mcp-clients", n);
                    crate::tray::refresh(&count_app);
                },
            )
            .await;
        });
        tracing::info!("MCP: listening for AI clients on 127.0.0.1:{port}");
        Ok(Running { task, port })
    })();
    match result {
        Ok(r) => {
            *running = Some(r);
            *lock(&at.last_error) = None;
            drop(running);
            crate::tray::refresh(app);
            Ok(())
        }
        Err(e) => {
            tracing::error!("MCP: could not start the listener: {e}");
            *lock(&at.last_error) = Some(e.clone());
            Err(e)
        }
    }
}

/// Stop the listener, disconnect clients and reject open approval cards.
pub fn stop(app: &AppHandle) {
    let state = app.state::<AppState>();
    let at = &state.attach;
    if let Some(r) = lock(&at.running).take() {
        r.task.abort();
        // Remove the run file only if it is ours (another ParVu may own it now).
        let path = mcp::run_file();
        if let Ok(text) = std::fs::read_to_string(&path)
            && let Ok(info) = serde_json::from_str::<RunInfo>(&text)
            && info.port == r.port
            && info.pid == std::process::id()
        {
            let _ = std::fs::remove_file(&path);
        }
        tracing::info!("MCP: stopped listening");
    }
    lock(&at.pending).clear();
    at.clients.store(0, Ordering::SeqCst);
    let _ = app.emit("mcp-clients", 0usize);
    crate::tray::refresh(app);
}

/// The app as seen by an attached MCP server.
struct TauriHost {
    app: AppHandle,
    /// Work on this window; `None` follows the window focused last.
    window: Option<String>,
}

/// A host pinned to one window (the built-in assistant of that window).
pub fn host_for_window(app: &AppHandle, label: &str) -> Arc<dyn AttachHost> {
    Arc::new(TauriHost { app: app.clone(), window: Some(label.to_string()) })
}

impl AttachHost for TauriHost {
    fn workspace(&self) -> Result<(String, Arc<Mutex<Workspace>>), String> {
        let state = self.app.state::<AppState>();
        let windows = self.app.webview_windows();
        if let Some(label) = &self.window {
            if !windows.contains_key(label) {
                return Err("The ParVu window was closed.".into());
            }
            let ws = state.workspace(label).map_err(|e| e.message)?;
            return Ok((label.clone(), ws));
        }
        let mut label = lock(&state.attach.active_window).clone();
        if !windows.contains_key(&label) {
            let mut labels: Vec<_> = windows.keys().cloned().collect();
            labels.sort();
            label = labels.into_iter().next().ok_or("No ParVu window is open.")?;
        }
        let ws = state.workspace(&label).map_err(|e| e.message)?;
        Ok((label, ws))
    }

    fn propose(&self, mut proposal: Proposal) -> Pin<Box<dyn Future<Output = Decision> + Send>> {
        let state = self.app.state::<AppState>();
        let id = state.attach.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        proposal.id = id;
        let (tx, rx) = oneshot::channel();
        lock(&state.attach.pending).insert(id, tx);
        let _ = self.app.emit_to(proposal.window.as_str(), "mcp-proposal", &proposal);
        call_attention(&self.app, &proposal.window);
        let app = self.app.clone();
        let window = proposal.window.clone();
        Box::pin(async move {
            match tokio::time::timeout(mcp::APPROVAL_TIMEOUT, rx).await {
                Ok(Ok(true)) => Decision::Approved,
                Ok(_) => Decision::Rejected,
                Err(_) => {
                    lock(&app.state::<AppState>().attach.pending).remove(&id);
                    let _ = app.emit_to(window.as_str(), "mcp-proposal-closed", id);
                    Decision::TimedOut
                }
            }
        })
    }

    fn changed(&self, window: &str, tab_id: u64) {
        let _ = self.app.emit_to(window, "mcp-changed", tab_id);
    }

    fn formulas_changed(&self, summary: &parvu_core::TabSummary) {
        crate::commands::remember_formulas(summary);
    }

    fn custom_dir(&self) -> Option<std::path::PathBuf> {
        Some(lock(&self.app.state::<AppState>().custom).dir().to_path_buf())
    }

    fn offer_custom(&self, window: &str, offer: CustomOffer) {
        let _ = self.app.emit_to(window, "custom-proposal", &offer);
        call_attention(&self.app, window);
    }
}

// ---------------------------------------------------------------- commands

#[tauri::command(rename_all = "snake_case")]
pub fn mcp_status(app: AppHandle) -> McpStatus {
    status(&app)
}

/// Switch AI clients on or off; remembered in the settings.
#[tauri::command(rename_all = "snake_case")]
pub fn mcp_set_enabled(app: AppHandle, enabled: bool) -> ApiResult<McpStatus> {
    {
        let state = app.state::<AppState>();
        let mut cfg = lock(&state.config);
        let mut values = serde_json::Map::new();
        values.insert("mcp_allow_clients".into(), enabled.into());
        cfg.settings.update(values)?;
    }
    if enabled {
        start(&app).map_err(|e| ApiError::new("mcp", e))?;
    } else {
        stop(&app);
    }
    Ok(status(&app))
}

/// The user's answer to an approval card. `false` when the request is gone
/// (the client disconnected or the card timed out).
#[tauri::command(rename_all = "snake_case")]
pub fn mcp_decide(app: AppHandle, id: u64, approve: bool) -> bool {
    let state = app.state::<AppState>();
    let sender = lock(&state.attach.pending).remove(&id);
    sender.is_some_and(|tx| tx.send(approve).is_ok())
}
