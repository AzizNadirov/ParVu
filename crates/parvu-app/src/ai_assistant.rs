//! The built-in AI assistant (plans/AI_PLAN.md §4, milestones B4–B5).
//!
//! * Providers live in `~/.ParVu/ai/providers.json`, which never holds keys.
//!   Keys are kept in the system keychain (service `ParVu`, account
//!   `ai/<provider id>`), or read from the environment variable the provider
//!   names when there is no keychain.
//! * The assistant's tools are the attached MCP server's, reached over an
//!   in-memory MCP connection and pinned to the asking window. Its changes
//!   therefore show the same approval cards as Claude Desktop's.
//! * One chat history per tab, in memory. Each question runs as a task that
//!   streams `ai-event`s to its window and ends with `ai-finished`;
//!   `ai_cancel` stops it.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use parvu_ai::{Agent, AgentEvent, Message, Provider, ProviderConfig, Secret, ToolBackend, ToolOutput, ToolSpec};
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::error::{ApiError, ApiResult};
use crate::mcp::ParvuMcp;
use crate::state::{AppState, lock};

const KEYCHAIN_SERVICE: &str = "ParVu";

/// Tool calls allowed for one question.
const MAX_TOOL_CALLS: usize = 15;

// ---------------------------------------------------------------- providers

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ProvidersFile {
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub default: Option<String>,
}

pub fn providers_path() -> PathBuf {
    parvu_config::paths::legacy_app_dir().join("ai").join("providers.json")
}

pub fn load_providers() -> ProvidersFile {
    std::fs::read_to_string(providers_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_providers(f: &ProvidersFile) -> ApiResult<()> {
    let path = providers_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| ApiError::new("ai", e.to_string()))?;
    }
    let text = serde_json::to_string_pretty(f).map_err(|e| ApiError::new("ai", e.to_string()))?;
    std::fs::write(&path, text).map_err(|e| ApiError::new("ai", format!("{}: {e}", path.display())))
}

/// Where a provider's key comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeySource {
    Keychain,
    Environment,
    None,
}

fn keychain_entry(id: &str) -> Option<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, &format!("ai/{id}")).ok()
}

/// How long the keychain probe may take. Without a Secret Service the
/// D-Bus calls behind it can wait indefinitely.
const KEYCHAIN_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Whether a system keychain works: a lookup of an entry that doesn't
/// exist must answer "no entry" (without a Secret Service, for example,
/// setting up the store succeeds but every lookup fails). Checked once, on
/// its own thread with a time limit: a probe that doesn't answer in time
/// counts as no keychain.
pub fn keychain_available() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        probe_with_timeout(KEYCHAIN_PROBE_TIMEOUT, || {
            keyring::Entry::store_status().is_ok()
                && keychain_entry("__probe__")
                    .is_some_and(|e| matches!(e.get_password(), Ok(_) | Err(keyring::Error::NoEntry)))
        })
    })
}

/// Run `probe` on a separate thread; `false` if it doesn't finish within
/// `timeout` (the thread is left to finish or hang on its own).
fn probe_with_timeout(timeout: std::time::Duration, probe: impl FnOnce() -> bool + Send + 'static) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new().name("keychain-probe".into()).spawn(move || {
        let _ = tx.send(probe());
    });
    if spawned.is_err() {
        return false;
    }
    match rx.recv_timeout(timeout) {
        Ok(ok) => ok,
        Err(_) => {
            tracing::warn!("the system keychain did not answer within {timeout:?}; API keys can't be stored there");
            false
        }
    }
}

fn key_for(cfg: &ProviderConfig) -> (Option<Secret>, KeySource) {
    if keychain_available()
        && let Some(e) = keychain_entry(&cfg.id)
        && let Ok(k) = e.get_password()
        && !k.trim().is_empty()
    {
        return (Some(Secret::new(k)), KeySource::Keychain);
    }
    if let Some(var) = cfg.key_env.as_deref().filter(|v| !v.is_empty())
        && let Ok(k) = std::env::var(var)
        && !k.trim().is_empty()
    {
        return (Some(Secret::new(k)), KeySource::Environment);
    }
    (None, KeySource::None)
}

#[derive(Debug, Serialize)]
pub struct ProviderView {
    pub config: ProviderConfig,
    pub key_source: KeySource,
    pub is_default: bool,
}

#[derive(Debug, Serialize)]
pub struct ProvidersInfo {
    pub providers: Vec<ProviderView>,
    pub keychain: bool,
}

fn providers_info() -> ProvidersInfo {
    let f = load_providers();
    let providers = f
        .providers
        .iter()
        .map(|c| ProviderView {
            key_source: key_for(c).1,
            is_default: f.default.as_deref() == Some(c.id.as_str()),
            config: c.clone(),
        })
        .collect();
    ProvidersInfo { providers, keychain: keychain_available() }
}

// The provider commands talk to the system keychain (D-Bus on Linux), which
// can be slow or hang: they are async and do the work on the blocking pool,
// never on the window's main thread.
#[tauri::command(rename_all = "snake_case")]
pub async fn ai_providers() -> ApiResult<ProvidersInfo> {
    blocking(providers_info).await
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> ApiResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| ApiError::new("app", e.to_string()))
}

/// Add or replace a provider. `key`, when given, goes to the keychain only;
/// an empty string removes the stored key.
#[tauri::command(rename_all = "snake_case")]
pub async fn ai_save_provider(config: ProviderConfig, key: Option<String>, make_default: bool) -> ApiResult<ProvidersInfo> {
    blocking(move || save_provider(config, key, make_default)).await?
}

fn save_provider(config: ProviderConfig, key: Option<String>, make_default: bool) -> ApiResult<ProvidersInfo> {
    let id = config.id.trim().to_string();
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(ApiError::new("ai", "The provider id may use letters, digits, '-' and '_'."));
    }
    if let Some(k) = key {
        let entry = keychain_entry(&id)
            .filter(|_| keychain_available())
            .ok_or_else(|| ApiError::new("ai", "No system keychain is available; set the key variable instead."))?;
        if k.trim().is_empty() {
            let _ = entry.delete_credential();
        } else {
            entry.set_password(k.trim()).map_err(|e| ApiError::new("ai", format!("Keychain: {e}")))?;
        }
    }
    let mut f = load_providers();
    let config = ProviderConfig { id: id.clone(), ..config };
    match f.providers.iter_mut().find(|p| p.id == id) {
        Some(p) => *p = config,
        None => f.providers.push(config),
    }
    if make_default || f.default.is_none() {
        f.default = Some(id);
    }
    save_providers(&f)?;
    Ok(providers_info())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn ai_delete_provider(id: String) -> ApiResult<ProvidersInfo> {
    blocking(move || delete_provider(id)).await?
}

fn delete_provider(id: String) -> ApiResult<ProvidersInfo> {
    let mut f = load_providers();
    f.providers.retain(|p| p.id != id);
    if f.default.as_deref() == Some(id.as_str()) {
        f.default = f.providers.first().map(|p| p.id.clone());
    }
    if keychain_available()
        && let Some(e) = keychain_entry(&id)
    {
        let _ = e.delete_credential();
    }
    save_providers(&f)?;
    Ok(providers_info())
}

fn provider(id: &str) -> ApiResult<Provider> {
    let f = load_providers();
    let cfg = f
        .providers
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| ApiError::new("ai", format!("No AI provider \"{id}\". Add one in AI → AI providers.")))?;
    let (key, _) = key_for(&cfg);
    Provider::new(cfg, key).map_err(|e| ApiError::new("ai", e.to_string()))
}

/// Send the smallest request; returns the model that answered.
#[tauri::command(rename_all = "snake_case")]
pub async fn ai_test_provider(id: String) -> ApiResult<String> {
    let p = provider(&id)?;
    p.test().await.map_err(|e| ApiError::new("ai", e.to_string()))
}

// ---------------------------------------------------------------- tools

/// The attached MCP server's tools, over an in-memory MCP connection.
struct McpTools {
    client: rmcp::service::RunningService<rmcp::RoleClient, ClientConfig>,
    specs: Vec<ToolSpec>,
}

impl McpTools {
    async fn connect(app: &AppHandle, window: &str) -> Result<Self, String> {
        let (server_io, client_io) = tokio::io::duplex(1 << 20);
        let server = ParvuMcp::attached(crate::mcp_attach::host_for_window(app, window));
        tauri::async_runtime::spawn(async move {
            if let Ok(running) = server.serve(server_io).await {
                let _ = running.waiting().await;
            }
        });
        let mut me = Implementation::new("parvu-assistant", env!("CARGO_PKG_VERSION"));
        me.title = Some("ParVu assistant".into());
        let client = ClientConfig::new(ClientCapabilities::default(), me)
            .serve(client_io)
            .await
            .map_err(|e| e.to_string())?;
        let specs = client
            .list_all_tools()
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|t| ToolSpec {
                name: t.name.to_string(),
                description: t.description.map(|d| d.to_string()).unwrap_or_default(),
                input_schema: Value::Object((*t.input_schema).clone()),
            })
            .collect();
        Ok(Self { client, specs })
    }
}

impl ToolBackend for McpTools {
    fn specs(&self) -> Vec<ToolSpec> {
        self.specs.clone()
    }

    fn call<'a>(&'a self, name: &'a str, input: Value) -> Pin<Box<dyn Future<Output = ToolOutput> + Send + 'a>> {
        Box::pin(async move {
            let Value::Object(args) = input else {
                return ToolOutput { text: "Tool input must be a JSON object.".into(), is_error: true };
            };
            match self.client.call_tool(CallToolRequestParams::new(name.to_string()).with_arguments(args)).await {
                Ok(r) => {
                    let content = serde_json::to_value(&r.content).unwrap_or(Value::Null);
                    let text = content
                        .as_array()
                        .map(|blocks| blocks.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"))
                        .unwrap_or_default();
                    ToolOutput { text, is_error: r.is_error == Some(true) }
                }
                Err(e) => ToolOutput { text: e.to_string(), is_error: true },
            }
        })
    }
}

// ---------------------------------------------------------------- chats and runs

/// Chats and running questions, one set per app (in [`AppState`]).
#[derive(Default)]
pub struct Assistant {
    /// (window, tab) → conversation sent to the model.
    chats: Mutex<HashMap<(String, u64), Vec<Message>>>,
    runs: Mutex<HashMap<u64, tauri::async_runtime::JoinHandle<()>>>,
    next_run: AtomicU64,
}

const SYSTEM_PROMPT: &str = "You are the data assistant inside ParVu, a desktop app for viewing and querying \
Parquet, CSV, JSON and Excel files with DuckDB. The user's open tables are tabs; each table name means the \
tab's current result, including the filters and other steps already applied.

How to work:
- Inspect before answering: describe_table, sample_rows, value_counts, column_stats, and query (one DuckDB \
SELECT over the open tables by name). Never invent column names; check them first.
- Quote identifiers that contain spaces or capitals in double quotes.
- To change a table (filter, sort, group by, pivot, new columns, fill missing values…), call apply_operation \
with a short rationale. Group by, pivot, unpivot, transpose, value counts, describe, join and append \
open their result as a new table (its name is in the answer) and leave the source as it was. For a live computed column (it updates when cells change), call set_formula with \
a formula such as [qty] * [price], [price] / SUM([price]) (a column total) or SUM([q1:q4]) (across a row). \
The user approves or rejects each change in ParVu. If they reject it, don't retry the same \
change; ask what they want instead. Use undo only when the user asks.
- The user may have defined custom functions and operations: list_custom_tools shows them and how to call \
them (apply_operation, or set_formula, where a summary function is a column total like SUM); custom functions \
also work by name inside query. To suggest a new \
one, call propose_custom_tool with a full definition and examples; ParVu checks it and offers it to the user \
in its editor. Only the user saves it, so don't use it until they have.
- Opening and saving files is up to the user.
- Answer briefly. Use Markdown tables for tabular results and show the SQL you ran when it helps.";

/// The table snapshot that starts each question (plans/AI_PLAN.md §9): it
/// goes in the user message so the system prompt and tools stay cacheable.
fn context_for(app: &AppHandle, window: &str, tab_id: Option<u64>) -> String {
    let state = app.state::<AppState>();
    let Ok(ws) = state.workspace(window) else { return String::new() };
    let (session, active) = {
        let ws = lock(&ws);
        let active = tab_id.and_then(|id| ws.tab_name(id).ok().map(str::to_string));
        (ws.read_session(), active)
    };
    let Ok(ro) = session else { return String::new() };
    let mut out = String::from("[ParVu context]\n");
    if ro.tables().is_empty() {
        out.push_str("No tables are open.\n");
        return out;
    }
    out.push_str(&format!("Open tables: {}\n", ro.tables().join(", ")));
    if let Some(name) = active
        && let Ok(info) = ro.describe_table(&name)
    {
        let cols: Vec<String> = info.columns.iter().take(80).map(|c| format!("{} {}", c.name, c.dtype)).collect();
        out.push_str(&format!("Active table: {} ({} rows)\nColumns: {}", info.name, info.rows, cols.join(", ")));
        if info.columns.len() > 80 {
            out.push_str(&format!(" … and {} more (use describe_table)", info.columns.len() - 80));
        }
        out.push('\n');
        if !info.steps.is_empty() {
            let steps: Vec<String> = info.steps.iter().map(|s| format!("{} {:?}", s.key, s.args)).collect();
            out.push_str(&format!("Applied steps: {}\n", steps.join("; ")));
        }
        if let Ok(head) = ro.sample(&name, parvu_core::readonly::SampleRows::Head, 3) {
            let rows: Vec<String> = head
                .rows
                .iter()
                .map(|r| r.iter().map(|c| c.as_deref().unwrap_or("NULL").chars().take(60).collect::<String>()).collect::<Vec<_>>().join(" | "))
                .collect();
            out.push_str(&format!("First rows:\n{}\n", rows.join("\n")));
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
struct RunEvent {
    run_id: u64,
    tab_id: Option<u64>,
    event: AgentEvent,
}

#[derive(Debug, Clone, Serialize)]
struct RunFinished {
    run_id: u64,
    tab_id: Option<u64>,
    error: Option<String>,
    cancelled: bool,
}

/// Ask the assistant about the window's tables; returns the run id. Events
/// follow on `ai-event` and `ai-finished`.
#[tauri::command(rename_all = "snake_case")]
pub async fn ai_ask(
    app: AppHandle,
    window: WebviewWindow,
    provider_id: String,
    tab_id: Option<u64>,
    question: String,
) -> ApiResult<u64> {
    if question.trim().is_empty() {
        return Err(ApiError::new("ai", "Type a question first."));
    }
    let provider = provider(&provider_id)?;
    let label = window.label().to_string();
    let state = app.state::<AppState>();
    let run_id = state.assistant.next_run.fetch_add(1, Ordering::SeqCst) + 1;
    let key = (label.clone(), tab_id.unwrap_or(0));
    let mut history = lock(&state.assistant.chats).get(&key).cloned().unwrap_or_default();
    let app2 = app.clone();

    let task = tauri::async_runtime::spawn(async move {
        let emit_event = |event: AgentEvent| {
            let _ = app2.emit_to(label.as_str(), "ai-event", RunEvent { run_id, tab_id, event });
        };
        let result: Result<(), String> = async {
            let tools = McpTools::connect(&app2, &label).await?;
            let context = context_for(&app2, &label, tab_id);
            let agent = Agent { provider: &provider, tools: &tools, system: SYSTEM_PROMPT.into(), max_tool_calls: MAX_TOOL_CALLS };
            let msg = Message::user_text(format!("{context}\n{}", question.trim()));
            agent.ask(&mut history, msg, &emit_event).await.map_err(|e| e.to_string())?;
            let _ = tools.client.cancel().await;
            Ok(())
        }
        .await;
        let st = app2.state::<AppState>();
        if result.is_ok() {
            lock(&st.assistant.chats).insert(key, history);
        }
        lock(&st.assistant.runs).remove(&run_id);
        let error = result.err().map(|e| parvu_ai::redact::redact(&e));
        if let Some(e) = &error {
            tracing::warn!("AI assistant: {e}");
        }
        let _ = app2.emit_to(label.as_str(), "ai-finished", RunFinished { run_id, tab_id, error, cancelled: false });
    });
    lock(&state.assistant.runs).insert(run_id, task);
    Ok(run_id)
}

/// Stop a running question. Its partial answer is not kept in the chat.
#[tauri::command(rename_all = "snake_case")]
pub fn ai_cancel(app: AppHandle, window: WebviewWindow, run_id: u64, tab_id: Option<u64>) -> bool {
    let state = app.state::<AppState>();
    let task = lock(&state.assistant.runs).remove(&run_id);
    let Some(task) = task else { return false };
    task.abort();
    let _ = window.emit_to(window.label(), "ai-finished", RunFinished { run_id, tab_id, error: None, cancelled: true });
    true
}

/// Forget the conversation of a tab.
#[tauri::command(rename_all = "snake_case")]
pub fn ai_clear_chat(app: AppHandle, window: WebviewWindow, tab_id: Option<u64>) {
    let state = app.state::<AppState>();
    lock(&state.assistant.chats).remove(&(window.label().to_string(), tab_id.unwrap_or(0)));
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_hanging_keychain_probe_counts_as_no_keychain() {
        let short = std::time::Duration::from_millis(100);
        assert!(super::probe_with_timeout(short, || true));
        assert!(!super::probe_with_timeout(short, || false));
        let start = std::time::Instant::now();
        assert!(!super::probe_with_timeout(short, || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            true
        }));
        assert!(start.elapsed() < std::time::Duration::from_secs(2), "the caller doesn't wait for a hung probe");
    }
}
