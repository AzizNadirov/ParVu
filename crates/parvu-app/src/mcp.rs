//! `parvu mcp`: ParVu as a Model Context Protocol server over stdio, without
//! a window (plans/AI_PLAN.md §19). AI clients such as Claude Code or Claude
//! Desktop start it and call its tools to open data files and query them
//! with DuckDB.
//!
//! Safety:
//! * files are opened only from folders given with `--allow-dir` (and files
//!   named on the command line);
//! * queries go through `parvu_core::readonly`, which allows one SELECT on
//!   the open tables and refuses anything that reads other files;
//! * table operations change only this session's views, never the files;
//! * `export` exists only with `--allow-write` and writes inside the allowed
//!   folders.
//!
//! stdout carries the protocol, so nothing else may print there; logs go to
//! the usual log file.
//!
//! Attached mode (`parvu mcp --attach`): when the user allows AI clients in
//! the app (AI → Connect an AI client), the app listens on 127.0.0.1 with a
//! random token written to `~/.ParVu/run/mcp.json`. `parvu mcp --attach` is a
//! stdio bridge to that listener (`bridge.rs`), so clients see the user's
//! open tabs. Changes there go through an approval card in the window
//! ([`AttachHost`]).

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use parvu_core::Workspace;
use parvu_core::export::{ExportFormat, ExportOptions};
use parvu_core::ops::{Operation, StepLabel};
use parvu_core::page::{Cell, ColumnMeta};
use parvu_core::readonly::{QueryRows, ReadSession, SampleRows};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, RequestMetaObject, ServerCapabilities, ServerConfig};
use rmcp::service::{Peer, RoleServer};
use rmcp::{ErrorData, ServerHandler, ServiceExt, schemars, tool, tool_handler, tool_router};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::net::TcpListener;
use serde::{Deserialize, Serialize};
use serde_json::json;

const QUERY_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_QUERY_ROWS: usize = 1000;
const MAX_SAMPLE_ROWS: usize = 100;

const USAGE: &str = "\
Usage: parvu mcp [--allow-dir DIR]... [--allow-write] [FILE]...
       parvu mcp --attach

Runs ParVu as an MCP server on stdin/stdout for AI clients.
  --allow-dir DIR   folder the client may open files from (repeatable)
  --allow-write     allow the export tool to write files into those folders
  FILE              files to open at start (always allowed)
  --attach          connect to the ParVu window instead (AI → Connect an AI
                    client must be switched on there); the first tool call
                    starts ParVu in the background (in the tray) when it
                    isn't running";

/// How long an approval card waits for the user.
pub const APPROVAL_TIMEOUT: Duration = Duration::from_secs(600);

/// Command-line options of `parvu mcp`.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Options {
    pub allow_dirs: Vec<PathBuf>,
    pub allow_write: bool,
    pub files: Vec<PathBuf>,
    pub attach: bool,
}

impl Options {
    /// Parse the arguments after `mcp`.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut o = Options::default();
        let mut args = args.into_iter();
        while let Some(a) = args.next() {
            match a.as_str() {
                "--allow-dir" => {
                    let dir = args.next().ok_or("--allow-dir needs a folder")?;
                    o.allow_dirs.push(expand_home(&dir));
                }
                "--allow-write" => o.allow_write = true,
                "--attach" => o.attach = true,
                "-h" | "--help" => return Err(USAGE.into()),
                s if s.starts_with("--") => return Err(format!("Unknown option {s}\n\n{USAGE}")),
                file => o.files.push(expand_home(file)),
            }
        }
        if o.attach && (o.allow_write || !o.allow_dirs.is_empty() || !o.files.is_empty()) {
            return Err(format!("--attach takes no other options: the window decides what is open\n\n{USAGE}"));
        }
        Ok(o)
    }
}

fn expand_home(p: &str) -> PathBuf {
    match (p.strip_prefix("~/").or(if p == "~" { Some("") } else { None }), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(p),
    }
}

/// Entry point for `parvu mcp …`; returns the process exit code.
pub fn run(args: Vec<String>) -> i32 {
    let options = match Options::parse(args) {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("{msg}");
            return 2;
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("parvu mcp: {e}");
            return 1;
        }
    };
    if options.attach {
        return runtime.block_on(async {
            match crate::bridge::run().await {
                Ok(()) => 0,
                Err(e) => {
                    tracing::warn!("parvu mcp --attach: {e}");
                    eprintln!("parvu mcp --attach: {e}");
                    1
                }
            }
        });
    }
    runtime.block_on(async move {
        let server = match ParvuMcp::new(options) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("parvu mcp: {e}");
                return 1;
            }
        };
        let limit = std::env::var("PARVU_MEMORY_LIMIT_MB").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(1024);
        crate::memguard::start(move || limit, |_| {});
        tracing::info!("MCP server starting");
        match server.serve(rmcp::transport::stdio()).await {
            Ok(running) => {
                let _ = running.waiting().await;
                0
            }
            Err(e) => {
                tracing::error!("MCP server failed: {e}");
                eprintln!("parvu mcp: {e}");
                1
            }
        }
    })
}

/// The running app, as seen by an attached server: the focused window's
/// workspace, approval cards and refreshes. Implemented in `mcp_attach.rs`.
pub trait AttachHost: Send + Sync + 'static {
    /// Label and workspace of the window the client works on.
    fn workspace(&self) -> Result<(String, Arc<Mutex<Workspace>>), String>;
    /// Show `proposal` in its window and wait for the user's answer.
    fn propose(&self, proposal: Proposal) -> Pin<Box<dyn Future<Output = Decision> + Send>>;
    /// A tab of `window` changed; the window reloads it.
    fn changed(&self, window: &str, tab_id: u64);
    /// A formula was set on a tab: remember the formulas for its file.
    fn formulas_changed(&self, _summary: &parvu_core::TabSummary) {}
    /// The folder of the user's custom tools, whose names a proposal may
    /// not take.
    fn custom_dir(&self) -> Option<PathBuf> {
        None
    }
    /// Show a custom tool an AI client proposed in `window`: the user may
    /// open it in the editor, review it and save it. Nothing is saved here.
    fn offer_custom(&self, _window: &str, _offer: CustomOffer) {}
}

/// A custom tool an AI client proposed, offered to the user in the editor.
#[derive(Debug, Clone, Serialize)]
pub struct CustomOffer {
    /// The client's name ("claude-code", "ParVu assistant"…).
    pub client: String,
    /// The definition with its validation report.
    pub definition: parvu_core::custom::Definition,
    /// The definition it would replace, if any.
    pub replaces: Option<String>,
    pub rationale: Option<String>,
    pub passed: bool,
}

/// A change an attached client asks for, shown as an approval card.
#[derive(Debug, Clone, Serialize)]
pub struct Proposal {
    /// Set by the host.
    pub id: u64,
    pub window: String,
    /// The client's name from its handshake ("claude-code", "Claude Desktop"…).
    pub client: String,
    pub tab_id: u64,
    pub table: String,
    /// "operation" or "undo".
    pub kind: &'static str,
    pub label: StepLabel,
    pub rationale: Option<String>,
    pub sql: Option<String>,
    pub rows_before: u64,
    pub rows_after: Option<u64>,
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<Cell>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Approved,
    Rejected,
    TimedOut,
}

#[derive(Clone)]
enum Source {
    /// `parvu mcp`: its own workspace and folder rules.
    Headless { ws: Arc<Mutex<Workspace>>, allow_dirs: Arc<Vec<PathBuf>>, allow_write: bool, custom_dir: Option<PathBuf> },
    /// Inside the app: the user's window.
    Attached(Arc<dyn AttachHost>),
}

/// The server. Headless it owns one workspace for the whole session;
/// attached it works on the focused window of the running app.
#[derive(Clone)]
pub struct ParvuMcp {
    source: Source,
    tool_router: ToolRouter<Self>,
}

// ---------- tool parameters ----------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct OpenFileArgs {
    /// Path of a Parquet, CSV, TSV, JSON / JSON Lines or Excel file inside an allowed folder.
    pub path: String,
    /// For an Excel workbook: the sheet to open (default: the first visible one).
    /// The result lists the workbook's sheets.
    #[serde(default)]
    pub sheet: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TableArgs {
    /// Table name, as listed by list_tables.
    pub table: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SampleArgs {
    /// Table name.
    pub table: String,
    /// head (first rows), tail (last rows) or random (a repeatable sample).
    #[serde(default = "default_head")]
    pub mode: SampleRows,
    /// Number of rows (at most 100).
    #[serde(default = "default_ten")]
    pub n: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ColumnArgs {
    /// Table name.
    pub table: String,
    /// Column name.
    pub column: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ValueCountsArgs {
    /// Table name.
    pub table: String,
    /// Column name.
    pub column: String,
    /// How many of the most frequent values to return (at most 100).
    #[serde(default = "default_ten")]
    pub k: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueryArgs {
    /// One DuckDB SELECT statement over the open tables, referenced by name.
    pub sql: String,
    /// Maximum rows to return (default 100, at most 1000).
    #[serde(default = "default_hundred")]
    pub max_rows: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ApplyArgs {
    /// Table to change.
    pub table: String,
    /// The operation, e.g. {"op": "filter", "conditions": [{"column": "price", "op": "gt", "value": "10"}]}.
    pub operation: Operation,
    /// One short sentence for the user saying why (shown when they are asked to approve).
    #[serde(default)]
    pub rationale: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProposeCustomArgs {
    /// The definition, as JSON (see the tool description for the format).
    pub definition: serde_json::Value,
    /// The name of an existing custom tool this one would replace (an improved version).
    #[serde(default)]
    pub replaces: Option<String>,
    /// One short sentence for the user saying what it is for.
    #[serde(default)]
    pub rationale: Option<String>,
    /// Offer it to the user in ParVu's editor (window only). Default true; pass false
    /// while still correcting a failing proposal, so the user sees only the final one.
    #[serde(default)]
    pub offer: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetFormulaArgs {
    /// Table to change.
    pub table: String,
    /// Name of the formula column; an existing formula column of that name is changed.
    pub name: String,
    /// The formula in ParVu's expression language, e.g. "[qty] * [price]", "[price] / SUM([price])"
    /// (SUM of one column is its total), "SUM([q1:q4])" (across the columns q1..q4 of each row),
    /// "[total] - PREV([total])", "IF([qty] > 10, 'bulk', 'single')". [column] means a column of this table.
    pub formula: String,
    /// Show the column right after this one (default: at the end).
    #[serde(default)]
    pub after: Option<String>,
    /// One short sentence for the user saying why (shown when they are asked to approve).
    #[serde(default)]
    pub rationale: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExportArgs {
    /// Table to write.
    pub table: String,
    /// Output path inside an allowed folder; the extension picks the format
    /// (.csv, .parquet, .json, .jsonl, .xlsx, or .md for a Markdown table).
    pub path: String,
    /// Replace the file if it exists.
    #[serde(default)]
    pub overwrite: bool,
}

fn default_head() -> SampleRows {
    SampleRows::Head
}
fn default_ten() -> usize {
    10
}
fn default_hundred() -> usize {
    100
}

// ---------- results ----------

/// Rows as compact JSON: column names and types once, then rows as arrays.
#[derive(Serialize)]
struct RowsOut<'a> {
    columns: Vec<&'a str>,
    types: Vec<&'a str>,
    rows: &'a [Vec<Cell>],
    truncated: bool,
}

fn rows_json(r: &QueryRows) -> serde_json::Value {
    let out = RowsOut {
        columns: r.columns.iter().map(|c| c.name.as_str()).collect(),
        types: r.columns.iter().map(|c| c.dtype.as_str()).collect(),
        rows: &r.rows,
        truncated: r.truncated,
    };
    serde_json::to_value(out).unwrap_or_default()
}

fn columns_json(cols: &[ColumnMeta]) -> serde_json::Value {
    cols.iter().map(|c| json!({ "name": c.name, "type": c.dtype })).collect()
}

fn ok(value: serde_json::Value) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::success(vec![ContentBlock::json(value)?]))
}

/// A mistake the model can fix (unknown column, refused SQL…): returned as a
/// tool error so the model sees the message, not as a protocol error.
fn tool_error(msg: impl std::fmt::Display) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::error(vec![ContentBlock::text(msg.to_string())]))
}

macro_rules! try_tool {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(err) => return tool_error(err),
        }
    };
}

impl ParvuMcp {
    pub fn new(options: Options) -> Result<Self, String> {
        let mut allow_dirs = Vec::new();
        for d in &options.allow_dirs {
            let canon = d.canonicalize().map_err(|e| format!("--allow-dir {}: {e}", d.display()))?;
            if !canon.is_dir() {
                return Err(format!("--allow-dir {}: not a folder", d.display()));
            }
            allow_dirs.push(canon);
        }
        let temp = parvu_config::paths::user_cache_dir().join("duckdb");
        let mut ws = Workspace::new(100, Some(&temp)).map_err(|e| e.to_string())?;
        // Custom operations the user saved can be applied here too (tests
        // never read the user's folder).
        if !cfg!(test) {
            ws.set_custom(crate::custom::headless_library());
        }
        for f in &options.files {
            ws.open(f).map_err(|e| format!("{}: {e}", f.display()))?;
        }
        Ok(Self {
            source: Source::Headless {
                ws: Arc::new(Mutex::new(ws)),
                allow_dirs: Arc::new(allow_dirs),
                allow_write: options.allow_write,
                custom_dir: (!cfg!(test)).then(crate::custom_store::CustomStore::default_dir),
            },
            tool_router: Self::tool_router(),
        })
    }

    /// A server working on the running app's windows. Opening and exporting
    /// files stay with the user, so those tools are not offered.
    pub fn attached(host: Arc<dyn AttachHost>) -> Self {
        let mut tool_router = Self::tool_router();
        tool_router.remove_route("open_file");
        tool_router.remove_route("export");
        Self { source: Source::Attached(host), tool_router }
    }

    fn is_attached(&self) -> bool {
        matches!(self.source, Source::Attached(_))
    }

    fn workspace(&self) -> Result<Arc<Mutex<Workspace>>, String> {
        match &self.source {
            Source::Headless { ws, .. } => Ok(ws.clone()),
            Source::Attached(host) => host.workspace().map(|(_, ws)| ws),
        }
    }

    /// A read-only snapshot, taken under the lock and used without it.
    fn session(&self) -> Result<ReadSession, String> {
        let ws = self.workspace()?;
        let guard = ws.lock().unwrap_or_else(|p| p.into_inner());
        guard.read_session().map_err(|e| e.to_string())
    }

    fn allow_dirs(&self) -> &[PathBuf] {
        match &self.source {
            Source::Headless { allow_dirs, .. } => allow_dirs,
            Source::Attached(_) => &[],
        }
    }

    fn allow_write(&self) -> bool {
        matches!(self.source, Source::Headless { allow_write: true, .. })
    }

    /// The folder of the user's custom tools (read only, for the names a
    /// proposal may not take).
    fn custom_dir(&self) -> Option<PathBuf> {
        match &self.source {
            Source::Headless { custom_dir, .. } => custom_dir.clone(),
            Source::Attached(host) => host.custom_dir(),
        }
    }

    /// Run blocking DuckDB work off the async runtime.
    async fn blocking<T: Send + 'static>(
        f: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        tokio::task::spawn_blocking(f).await.map_err(|e| e.to_string())?
    }

    /// Run `f` on a read-only snapshot, off the async runtime. When it fails
    /// on a CSV value DuckDB couldn't parse, the workspace re-reads that
    /// column as text (see `Workspace::heal`) and `f` runs again.
    async fn read<T: Send + 'static>(
        &self,
        f: impl Fn(&ReadSession) -> parvu_core::Result<T> + Send + 'static,
    ) -> Result<T, String> {
        let ws = self.workspace()?;
        Self::blocking(move || {
            let mut tries = 0;
            loop {
                let ro = ws.lock().unwrap_or_else(|p| p.into_inner()).read_session().map_err(|e| e.to_string())?;
                match f(&ro) {
                    Ok(v) => return Ok(v),
                    Err(e) => {
                        let message = e.to_string();
                        drop(ro);
                        if tries < 8 && ws.lock().unwrap_or_else(|p| p.into_inner()).heal(&message) {
                            tries += 1;
                            continue;
                        }
                        return Err(message);
                    }
                }
            }
        })
        .await
    }

    /// `path` canonicalised, if it lies inside an allowed folder.
    fn allowed(&self, path: &Path) -> Result<PathBuf, String> {
        if self.allow_dirs().is_empty() {
            return Err("No folders are allowed. Start the server with --allow-dir <folder>.".into());
        }
        let canon = path.canonicalize().map_err(|e| format!("{}: {e}", path.display()))?;
        if self.allow_dirs().iter().any(|d| canon.starts_with(d)) {
            Ok(canon)
        } else {
            Err(format!("{} is outside the allowed folders: {}", path.display(), self.allowed_list()))
        }
    }

    fn allowed_list(&self) -> String {
        self.allow_dirs().iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join(", ")
    }

    fn tab_id(ws: &Workspace, table: &str) -> Result<u64, String> {
        ws.tab_ids()
            .into_iter()
            .find(|&id| ws.tab_name(id).is_ok_and(|n| n.eq_ignore_ascii_case(table)))
            .ok_or_else(|| format!("Unknown table \"{table}\"."))
    }

    fn instructions(&self) -> String {
        if self.is_attached() {
            return "You are connected to the user's running ParVu window. The tables are the tabs they have \
                    open, including the filters and other steps they applied.\n\
                    Inspect before answering: list_tables, describe_table, sample_rows, value_counts, column_stats.\n\
                    query runs one DuckDB SELECT over the open tables by name; file functions such as read_csv \
                    are refused. Quote identifiers with spaces in double quotes.\n\
                    apply_operation, set_formula and undo change the user's table only after they approve a card \
                    in ParVu, so give a short rationale. Group by, pivot, unpivot, transpose, value counts, describe, \
                    join and append open their result as a new tab. set_formula adds a live formula column, e.g. \
                    [price] / SUM([price]). If they reject it, don't retry the same change; ask them instead.\n\
                    list_custom_tools shows the custom functions and operations the user defined, and how to call \
                    them (apply_operation, set_formula, or by name inside query). propose_custom_tool checks a new definition and offers \
                    it to the user in ParVu's editor; only the user can save it.\n\
                    Files are opened and saved by the user in ParVu."
                .into();
        }
        let dirs = if self.allow_dirs().is_empty() { "none (opening files is disabled)".into() } else { self.allowed_list() };
        format!(
            "ParVu opens Parquet, CSV, JSON and Excel files as tables and queries them with DuckDB.\n\
             Start with list_tables, or open_file for a file in the allowed folders ({dirs}).\n\
             Inspect before answering: describe_table, sample_rows, value_counts, column_stats.\n\
             query runs one DuckDB SELECT over the open tables by name; file functions such as \
             read_csv are refused. Quote identifiers with spaces in double quotes.\n\
             apply_operation changes a table for this session only (filters, new columns…); undo \
             reverts it. Group by, pivot, unpivot, transpose, value counts, describe, join and \
             append open their result as a new table instead (named in the answer). Files on disk are never changed{}.\n\
             list_custom_tools shows the user's custom functions and operations and how to call them \
             (custom functions also work by name inside query); \
             propose_custom_tool only checks a new definition (it is never saved from here).",
            if self.allow_write() { ", except by export" } else { "" }
        )
    }
}

impl ParvuMcp {
    /// Apply `op` to tab `id`. A table operation (group by, pivot, join…)
    /// opens its result as a new table instead. Returns the changed or new
    /// tab and what to tell the AI.
    fn apply_now(ws: &Mutex<Workspace>, id: u64, op: &Operation) -> Result<(u64, serde_json::Value), String> {
        let mut ws = ws.lock().unwrap_or_else(|p| p.into_inner());
        if op.is_table_op() {
            use parvu_core::workspace::{Derivation, Materialized, RESULT_ROW_LIMIT, materialize};
            let plan = ws.plan_result(id, Derivation::Op { op: op.clone() }).map_err(|e| e.to_string())?;
            if materialize(ws.connection(), &plan).map_err(|e| e.to_string())? == Materialized::TooBig {
                return Err(format!(
                    "The result would have more than {RESULT_ROW_LIMIT} rows. Filter or aggregate the table first."
                ));
            }
            let label = plan.origin.label.clone();
            let source = plan.origin.source.clone();
            let new = ws.add_result(plan).map_err(|e| e.to_string())?;
            let s = ws.summary(new).map_err(|e| e.to_string())?;
            let page = ws.page(new, 1).map_err(|e| e.to_string())?;
            let first: Vec<_> = page.rows.into_iter().take(10).collect();
            return Ok((
                new,
                json!({
                    "new_table": s.name,
                    "from": source,
                    "step": { "step": label.key, "args": label.args },
                    "rows": s.total_rows,
                    "columns": columns_json(&page.columns),
                    "first_rows": first,
                    "note": format!("The result is a new table, {}; {source} is unchanged.", s.name),
                }),
            ));
        }
        let label = ws.apply(id, op).map_err(|e| e.to_string())?;
        let s = ws.summary(id).map_err(|e| e.to_string())?;
        let page = ws.page(id, 1).map_err(|e| e.to_string())?;
        let first: Vec<_> = page.rows.into_iter().take(10).collect();
        let out = json!({
            "table": s.name,
            "step": { "step": label.key, "args": label.args },
            "rows": s.total_rows,
            "columns": columns_json(&page.columns),
            "first_rows": first,
        });
        Ok((id, out))
    }

    /// Compile and set a formula; the first rows show the result.
    fn formula_now(ws: &Mutex<Workspace>, id: u64, args: &SetFormulaArgs) -> Result<(serde_json::Value, parvu_core::TabSummary), String> {
        let mut ws = ws.lock().unwrap_or_else(|p| p.into_inner());
        let f = crate::commands::compile_tab_formula(&ws, id, args.name.clone(), &args.formula, args.after.clone())
            .map_err(|e| e.message)?;
        let label = ws.set_formula(id, f).map_err(|e| e.to_string())?;
        let s = ws.summary(id).map_err(|e| e.to_string())?;
        let page = ws.page(id, 1).map_err(|e| e.to_string())?;
        let first: Vec<_> = page.rows.into_iter().take(10).collect();
        let out = json!({
            "table": s.name,
            "step": { "step": label.key, "args": label.args },
            "rows": s.total_rows,
            "columns": columns_json(&page.columns),
            "first_rows": first,
        });
        Ok((out, s))
    }

    fn undo_now(ws: &Mutex<Workspace>, id: u64) -> Result<serde_json::Value, String> {
        let mut ws = ws.lock().unwrap_or_else(|p| p.into_inner());
        let label = ws.undo(id).map_err(|e| e.to_string())?;
        let s = ws.summary(id).map_err(|e| e.to_string())?;
        Ok(json!({ "table": s.name, "undone": { "step": label.key, "args": label.args }, "rows": s.total_rows }))
    }
}

/// What `propose_custom_tool` takes, for the client.
const PROPOSE_DESCRIPTION: &str = "Propose a new custom function or operation for the user. ParVu validates it \
(a safety guard that refuses file functions and other tables, type checks, a shape check, and the examples as tests) \
and returns the report, so you can correct it and propose again. Nothing is saved: in the ParVu window the user is \
offered the definition in the editor, reviews it and saves it; the headless server only validates. Definition JSON: \
{\"kind\": \"function\"|\"operation\", \"name\": \"clean_phone\" (lower case, a-z 0-9 _), \"title\", \"description\", \
\"params\": [{\"name\": \"phone\", \"kind\": \"column\", \"type\": [\"text\"]}, {\"name\": \"prefix\", \"kind\": \"value\", \
\"dtype\": \"VARCHAR\", \"optional\": true, \"default\": \"+\"}] (operations start with {\"name\": \"t\", \"kind\": \"table\"}; \
{\"kind\": \"columns\", \"type\": [...], \"min\": 1, \"max\": 3} takes several columns and stands for a comma-separated list, \
allowed only in a select list, GROUP BY, PARTITION BY, DISTINCT ON, ORDER BY without ASC/DESC and function arguments; \
column type sets: any, number, integer, decimal, float, text, boolean, date, datetime, date_or_datetime, time, interval, \
json, list, struct; value dtypes: VARCHAR, BIGINT, INTEGER, DOUBLE, DECIMAL(18,3), BOOLEAN, DATE, TIMESTAMP, TIME, INTERVAL), \
\"result\": {\"kind\": \"column\"|\"value\"|\"table\", \"type\": [\"text\"]} (column: one value per row; value: one value \
for the whole column, an aggregate; table: operations), \"placement\": \"step\"|\"new_table\" (operations), \
\"body\": {\"language\": \"sql\"|\"expression\", \"source\": ...} (a function: one SQL expression of the parameters by \
name, e.g. \"upper(phone)\", or ParVu's expression language with [phone]; an operation: one SQL SELECT over the table \
parameters), \"examples\": [{\"input\": {\"columns\": [{\"name\": \"phone\", \"dtype\": \"VARCHAR\"}], \"rows\": [[\"555.0102\"], [null]]}, \
\"args\": {\"prefix\": \"+\"}, \"expected\": {\"columns\": [{\"name\": \"result\"}], \"rows\": [[\"+5550102\"], [null]]}}] \
(a function's input has one column per column parameter; a summary's expected has one row; an operation's input is its \
first table, args name the column chosen for each column parameter; a columns parameter's arg is a list of column names, \
in functions too). At least one example must pass; include one \
with empty values.";

impl ParvuMcp {
    async fn propose_impl(&self, args: ProposeCustomArgs, client: String) -> Result<CallToolResult, ErrorData> {
        let def = try_tool!(crate::custom::proposed_definition(args.definition));
        let dir = self.custom_dir();
        let replaces = args.replaces.clone();
        let (def, report) = try_tool!(
            Self::blocking(move || {
                let report = crate::custom::validate_proposal(dir.as_deref(), &def, replaces.as_deref());
                Ok((def, report))
            })
            .await
        );
        let passed = report.passed();
        let mut out = crate::custom::report_json(&report);
        match &self.source {
            Source::Attached(host) if args.offer.unwrap_or(true) => {
                let (window, _) = try_tool!(host.workspace());
                let mut definition = def;
                definition.report = Some(report);
                host.offer_custom(&window, CustomOffer { client, definition, replaces: args.replaces, rationale: args.rationale, passed });
                out["next"] = json!(if passed {
                    "All checks pass. The definition was offered to the user in ParVu's editor; they review it and \
                     save it themselves. It is not saved yet, so don't call it until they say so."
                } else {
                    "Some checks fail (see checks). It was offered to the user in ParVu's editor as it is; correct \
                     it and propose again (offer: false while correcting)."
                });
            }
            Source::Attached(_) => {
                out["next"] = json!("Not offered to the user (offer: false). Propose again with offer: true when it passes.");
            }
            Source::Headless { .. } => {
                out["next"] = json!(
                    "This server only validates custom tools; it never saves them. The user can create it in the \
                     ParVu app (Custom tools), or connect you to the ParVu window to offer it there."
                );
            }
        }
        ok(out)
    }

    async fn apply_impl(&self, args: ApplyArgs, client: String) -> Result<CallToolResult, ErrorData> {
        let ro = try_tool!(self.session());
        let table = args.table.clone();
        let op = args.operation.clone();
        try_tool!(Self::blocking(move || ro.check_operation(&table, &op).map_err(|e| e.to_string())).await);
        if let Source::Attached(host) = &self.source {
            let (window, ws) = try_tool!(host.workspace());
            let (table, op, rationale) = (args.table.clone(), args.operation.clone(), args.rationale.clone());
            let ws2 = ws.clone();
            let proposal = try_tool!(
                Self::blocking(move || {
                    let ws = ws2.lock().unwrap_or_else(|p| p.into_inner());
                    let id = Self::tab_id(&ws, &table)?;
                    let p = ws.preview_op(id, &op, 20).map_err(|e| e.to_string())?;
                    Ok(Proposal {
                        id: 0,
                        window,
                        client,
                        tab_id: id,
                        table: ws.tab_name(id).map_err(|e| e.to_string())?.to_string(),
                        kind: if op.is_table_op() { "new_table" } else { "operation" },
                        label: p.label,
                        rationale,
                        sql: Some(p.sql),
                        rows_before: p.rows_before,
                        rows_after: Some(p.rows_after),
                        columns: p.columns,
                        rows: p.rows,
                    })
                })
                .await
            );
            let (window, tab_id) = (proposal.window.clone(), proposal.tab_id);
            match host.propose(proposal).await {
                Decision::Approved => {}
                Decision::Rejected => return tool_error("The user rejected this change in ParVu. Ask them what they want instead of retrying it."),
                Decision::TimedOut => return tool_error("The user did not answer the approval request in ParVu."),
            }
            let (changed, out) = try_tool!(Self::blocking(move || Self::apply_now(&ws, tab_id, &args.operation)).await);
            host.changed(&window, changed);
            return ok(out);
        }
        let ws = try_tool!(self.workspace());
        let out = try_tool!(
            Self::blocking(move || {
                let id = {
                    let ws = ws.lock().unwrap_or_else(|p| p.into_inner());
                    Self::tab_id(&ws, &args.table)?
                };
                Self::apply_now(&ws, id, &args.operation).map(|(_, out)| out)
            })
            .await
        );
        ok(out)
    }

    async fn formula_impl(&self, args: SetFormulaArgs, client: String) -> Result<CallToolResult, ErrorData> {
        let ws = try_tool!(self.workspace());
        if let Source::Attached(host) = &self.source {
            let (window, _) = try_tool!(host.workspace());
            let ws2 = ws.clone();
            let (table, name, source, after, rationale) =
                (args.table.clone(), args.name.clone(), args.formula.clone(), args.after.clone(), args.rationale.clone());
            let proposal = try_tool!(
                Self::blocking(move || {
                    let ws = ws2.lock().unwrap_or_else(|p| p.into_inner());
                    let id = Self::tab_id(&ws, &table)?;
                    let f = crate::commands::compile_tab_formula(&ws, id, name.clone(), &source, after).map_err(|e| e.message)?;
                    let s = ws.summary(id).map_err(|e| e.to_string())?;
                    let exists = s.formulas.iter().any(|x| x.name == name);
                    let (columns, rows) = ws.preview_formula(id, &f, 20).map_err(|e| e.to_string())?;
                    Ok(Proposal {
                        id: 0,
                        window,
                        client,
                        tab_id: id,
                        table: s.name,
                        kind: "formula",
                        label: StepLabel::new(if exists { "step.formula_edit" } else { "step.formula_add" }, &[("column", &name)]),
                        rationale,
                        sql: Some(format!("= {}", f.source)),
                        rows_before: s.total_rows,
                        rows_after: Some(s.total_rows),
                        columns,
                        rows,
                    })
                })
                .await
            );
            let (window, tab_id) = (proposal.window.clone(), proposal.tab_id);
            match host.propose(proposal).await {
                Decision::Approved => {}
                Decision::Rejected => return tool_error("The user rejected this formula in ParVu. Ask them what they want instead of retrying it."),
                Decision::TimedOut => return tool_error("The user did not answer the approval request in ParVu."),
            }
            let (out, summary) = try_tool!(Self::blocking(move || Self::formula_now(&ws, tab_id, &args)).await);
            host.formulas_changed(&summary);
            host.changed(&window, tab_id);
            return ok(out);
        }
        let out = try_tool!(
            Self::blocking(move || {
                let id = {
                    let ws = ws.lock().unwrap_or_else(|p| p.into_inner());
                    Self::tab_id(&ws, &args.table)?
                };
                Self::formula_now(&ws, id, &args).map(|(out, _)| out)
            })
            .await
        );
        ok(out)
    }

    async fn undo_impl(&self, args: TableArgs, client: String) -> Result<CallToolResult, ErrorData> {
        let ws = try_tool!(self.workspace());
        if let Source::Attached(host) = &self.source {
            let (window, _) = try_tool!(host.workspace());
            let ws2 = ws.clone();
            let table = args.table.clone();
            let proposal = try_tool!(
                Self::blocking(move || {
                    let ws = ws2.lock().unwrap_or_else(|p| p.into_inner());
                    let id = Self::tab_id(&ws, &table)?;
                    let s = ws.summary(id).map_err(|e| e.to_string())?;
                    let label = s.steps.last().cloned().ok_or("There is nothing to undo on this table.")?;
                    Ok(Proposal {
                        id: 0,
                        window,
                        client,
                        tab_id: id,
                        table: s.name,
                        kind: "undo",
                        label,
                        rationale: None,
                        sql: None,
                        rows_before: s.total_rows,
                        rows_after: None,
                        columns: Vec::new(),
                        rows: Vec::new(),
                    })
                })
                .await
            );
            let (window, tab_id) = (proposal.window.clone(), proposal.tab_id);
            match host.propose(proposal).await {
                Decision::Approved => {}
                Decision::Rejected => return tool_error("The user declined the undo in ParVu."),
                Decision::TimedOut => return tool_error("The user did not answer the approval request in ParVu."),
            }
            let out = try_tool!(Self::blocking(move || Self::undo_now(&ws, tab_id)).await);
            host.changed(&window, tab_id);
            return ok(out);
        }
        let out = try_tool!(
            Self::blocking(move || {
                let id = {
                    let ws = ws.lock().unwrap_or_else(|p| p.into_inner());
                    Self::tab_id(&ws, &args.table)?
                };
                Self::undo_now(&ws, id)
            })
            .await
        );
        ok(out)
    }
}

/// The client's display name: from the request's metadata (clients using
/// the handshake-free protocol, such as current Claude Code), else from its
/// `initialize` handshake.
fn client_name(meta: &RequestMetaObject, peer: &Peer<RoleServer>) -> String {
    let info = meta.client_info().or_else(|| peer.peer_info().map(|i| i.client_info.clone()));
    info.map(|i| display_name(&i)).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "An AI client".into())
}

/// "Claude Code" rather than "claude-code": the title when given, else the
/// name with dashes turned into words.
fn display_name(i: &Implementation) -> String {
    if let Some(t) = i.title.as_deref().filter(|t| !t.trim().is_empty()) {
        return t.to_string();
    }
    let words: Vec<String> = i
        .name
        .split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        })
        .collect();
    if words.is_empty() { i.name.clone() } else { words.join(" ") }
}

#[tool_router]
impl ParvuMcp {
    #[tool(description = "List the open tables with their row and column counts.")]
    async fn list_tables(&self) -> Result<CallToolResult, ErrorData> {
        let out = try_tool!(
            self.read(|ro| {
                ro.tables()
                    .iter()
                    .map(|t| {
                        ro.describe_table(t)
                            .map(|i| json!({ "table": i.name, "rows": i.rows, "columns": i.columns.len(), "steps": i.steps.len() }))
                    })
                    .collect::<parvu_core::Result<Vec<_>>>()
            })
            .await
        );
        ok(json!({ "tables": out }))
    }

    #[tool(description = "Open a data file (Parquet, CSV, TSV, JSON, JSON Lines, Excel) from an allowed folder as a table. Returns the table name and columns.")]
    async fn open_file(&self, Parameters(args): Parameters<OpenFileArgs>) -> Result<CallToolResult, ErrorData> {
        let path = try_tool!(self.allowed(&expand_home(&args.path)));
        let ws = try_tool!(self.workspace());
        let out = try_tool!(
            Self::blocking(move || {
                let mut ws = ws.lock().unwrap_or_else(|p| p.into_inner());
                let csv = parvu_core::types::CsvOptions::default();
                let id = ws.open_sheet(&path, &csv, args.sheet.as_deref()).map_err(|e| e.to_string())?;
                let s = ws.summary(id).map_err(|e| e.to_string())?;
                let cols = ws.columns(id).map_err(|e| e.to_string())?;
                let mut out = json!({ "table": s.name, "rows": s.total_rows, "columns": columns_json(&cols) });
                if let Ok(v) = ws.sheets(id) {
                    out["sheet"] = json!(v.current);
                    out["sheets"] = json!(v.sheets.iter().map(|x| json!({ "name": x.name, "hidden": x.hidden })).collect::<Vec<_>>());
                }
                Ok(out)
            })
            .await
        );
        ok(out)
    }

    #[tool(description = "Columns with types, row count and applied steps of a table.")]
    async fn describe_table(&self, Parameters(args): Parameters<TableArgs>) -> Result<CallToolResult, ErrorData> {
        let info = try_tool!(self.read(move |ro| ro.describe_table(&args.table)).await);
        let steps: Vec<_> = info.steps.iter().map(|s| json!({ "step": s.key, "args": s.args })).collect();
        ok(json!({ "table": info.name, "rows": info.rows, "columns": columns_json(&info.columns), "steps": steps }))
    }

    #[tool(description = "First, last or random rows of a table.")]
    async fn sample_rows(&self, Parameters(args): Parameters<SampleArgs>) -> Result<CallToolResult, ErrorData> {
        let n = args.n.clamp(1, MAX_SAMPLE_ROWS);
        let rows = try_tool!(self.read(move |ro| ro.sample(&args.table, args.mode, n)).await);
        ok(rows_json(&rows))
    }

    #[tool(description = "Statistics for one column: rows, nulls, distinct values, min, max, mean and standard deviation.")]
    async fn column_stats(&self, Parameters(args): Parameters<ColumnArgs>) -> Result<CallToolResult, ErrorData> {
        let stats = try_tool!(self.read(move |ro| ro.column_stats(&args.table, &args.column)).await);
        ok(serde_json::to_value(stats).unwrap_or_default())
    }

    #[tool(description = "The most frequent values of a column with their counts.")]
    async fn value_counts(&self, Parameters(args): Parameters<ValueCountsArgs>) -> Result<CallToolResult, ErrorData> {
        let k = args.k.clamp(1, MAX_SAMPLE_ROWS);
        let rows = try_tool!(self.read(move |ro| ro.value_counts(&args.table, &args.column, k)).await);
        ok(rows_json(&rows))
    }

    #[tool(description = "Run one read-only DuckDB SELECT over the open tables (referenced by table name) and return the rows. Joins, CTEs, window functions and aggregates work, and so do the user's custom functions (see list_custom_tools); reading other files does not.")]
    async fn query(&self, Parameters(args): Parameters<QueryArgs>) -> Result<CallToolResult, ErrorData> {
        let max = args.max_rows.clamp(1, MAX_QUERY_ROWS);
        let rows = try_tool!(self.read(move |ro| ro.query(&args.sql, max, Some(QUERY_TIMEOUT))).await);
        ok(rows_json(&rows))
    }

    #[tool(description = "Apply a table operation (filter, sort, group by, pivot, melt, add column, fill missing values, window functions, …) as a new step on a table. Group by, pivot, melt, transpose, value counts, describe, join and append leave the table as it is and open their result as a new table (new_table in the answer). Returns the row count and the first rows. When connected to the ParVu window, the user approves the change first.")]
    async fn apply_operation(
        &self,
        Parameters(args): Parameters<ApplyArgs>,
        meta: RequestMetaObject,
        client: Peer<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.apply_impl(args, client_name(&meta, &client)).await
    }

    #[tool(description = "Add or change a formula column: a live column computed from each row by an expression in ParVu's language, which updates when cells change (unlike apply_operation's add column, cells stay editable). Use [column] for this table's columns; SUM([x]) alone is the column's total, SUM([a:c]) adds across columns a..c of the row, PREV([x]) / NEXT([x]) read neighbouring rows. When connected to the ParVu window, the user approves it first.")]
    async fn set_formula(
        &self,
        Parameters(args): Parameters<SetFormulaArgs>,
        meta: RequestMetaObject,
        client: Peer<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.formula_impl(args, client_name(&meta, &client)).await
    }

    #[tool(description = "Undo the last applied operation on a table. When connected to the ParVu window, the user approves it first.")]
    async fn undo(
        &self,
        Parameters(args): Parameters<TableArgs>,
        meta: RequestMetaObject,
        client: Peer<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.undo_impl(args, client_name(&meta, &client)).await
    }
    #[tool(description = "The custom functions and operations the user defined in ParVu: name, kind (row function, summary function or operation), typed signature, description, one example, and how to call each one (an apply_operation operation, or a set_formula formula; a summary function in a formula is a column total like SUM). A columns parameter takes {\"columns\": [names]}. A summary function of one column can also be a group_by aggregation: {\"column\": \"price\", \"func\": \"custom\", \"custom\": {\"name\": \"...\", \"values\": {...}}}.")]
    async fn list_custom_tools(&self) -> Result<CallToolResult, ErrorData> {
        let ws = try_tool!(self.workspace());
        let lib = ws.lock().unwrap_or_else(|p| p.into_inner()).custom().clone();
        ok(crate::custom::tool_listing(&lib))
    }

    #[tool(description = PROPOSE_DESCRIPTION)]
    async fn propose_custom_tool(
        &self,
        Parameters(args): Parameters<ProposeCustomArgs>,
        meta: RequestMetaObject,
        client: Peer<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        self.propose_impl(args, client_name(&meta, &client)).await
    }

    #[tool(description = "Write a table (with its applied steps) to a file in an allowed folder. Only available when the server runs with --allow-write.")]
    async fn export(&self, Parameters(args): Parameters<ExportArgs>) -> Result<CallToolResult, ErrorData> {
        if !self.allow_write() {
            return tool_error("Writing files is disabled. Start the server with --allow-write to allow export.");
        }
        let target = expand_home(&args.path);
        let parent = target.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let dir = try_tool!(self.allowed(parent));
        let Some(file_name) = target.file_name() else {
            return tool_error("The path needs a file name.");
        };
        let path = dir.join(file_name);
        if path.exists() && !args.overwrite {
            return tool_error(format!("{} exists; pass overwrite: true to replace it.", path.display()));
        }
        let ws = try_tool!(self.workspace());
        let out = try_tool!(
            Self::blocking(move || {
                let ws = ws.lock().unwrap_or_else(|p| p.into_inner());
                let id = Self::tab_id(&ws, &args.table)?;
                let options = ExportOptions { format: ExportFormat::for_path(&path), path: path.clone(), ..Default::default() };
                let rows = ws.export(id, &options).map_err(|e| e.to_string())?;
                Ok(json!({ "path": path.display().to_string(), "rows": rows }))
            })
            .await
        );
        ok(out)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ParvuMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("parvu", env!("CARGO_PKG_VERSION")))
            .with_instructions(self.instructions())
    }
}

// ---------- attached mode: run file, listener and bridge ----------

/// Where the app publishes its listener while AI clients are allowed.
pub fn run_file() -> PathBuf {
    parvu_config::paths::legacy_app_dir().join("run").join("mcp.json")
}

/// Contents of [`run_file`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunInfo {
    pub port: u16,
    pub token: String,
    pub pid: u32,
}

/// Write `info` so only the user can read it.
pub fn write_run_file(path: &Path, info: &RunInfo) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string(info).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    {
        use std::io::Write;
        let mut f = opts.open(&tmp)?;
        f.write_all(text.as_bytes())?;
    }
    std::fs::rename(&tmp, path)
}

/// A random token for the listener (hex, 256 bits).
pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS random number generator works");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn same_token(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Serve MCP sessions on `listener`. Each connection must send `token` and a
/// newline first; then it speaks MCP (JSON lines) with a fresh server from
/// `make`. `on_count` hears the number of connected clients.
pub async fn serve_tcp(
    listener: TcpListener,
    token: String,
    make: impl Fn() -> ParvuMcp + Send + Sync + 'static,
    on_count: impl Fn(usize) + Send + Sync + 'static,
) {
    let make = Arc::new(make);
    let on_count = Arc::new(on_count);
    let clients = Arc::new(AtomicUsize::new(0));
    // Sessions live in this set, so stopping the listener (dropping this
    // future) disconnects every client too.
    let mut sessions = tokio::task::JoinSet::new();
    loop {
        while sessions.try_join_next().is_some() {}
        let Ok((stream, peer)) = listener.accept().await else { continue };
        if !peer.ip().is_loopback() {
            continue;
        }
        let (make, on_count, clients, token) = (make.clone(), on_count.clone(), clients.clone(), token.clone());
        sessions.spawn(async move {
            let (read, write) = stream.into_split();
            let mut read = BufReader::new(read);
            let mut first = String::new();
            let got = tokio::time::timeout(Duration::from_secs(5), async {
                let mut limited = (&mut read).take(256);
                limited.read_line(&mut first).await
            })
            .await;
            if !matches!(got, Ok(Ok(n)) if n > 0) || !same_token(first.trim_end(), &token) {
                tracing::warn!("MCP: refused a connection with a wrong token");
                return;
            }
            on_count(clients.fetch_add(1, Ordering::SeqCst) + 1);
            tracing::info!("MCP: AI client connected");
            if let Ok(running) = make().serve((read, write)).await {
                let _ = running.waiting().await;
            }
            on_count(clients.fetch_sub(1, Ordering::SeqCst) - 1);
            tracing::info!("MCP: AI client disconnected");
        });
    }
}

/// Remove a listener file left by a ParVu that was killed. Called by the
/// app at start while it holds the instance lock (`instance.rs`), so no
/// other ParVu of this profile can be listening.
pub fn clear_stale_run_file() {
    let path = run_file();
    let stale = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str::<RunInfo>(&t).ok())
        .is_none_or(|info| info.pid != std::process::id());
    if stale {
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_options() {
        let o = Options::parse(["--allow-dir", "/data", "--allow-write", "a.csv"].map(String::from)).unwrap();
        assert_eq!(o.allow_dirs, [PathBuf::from("/data")]);
        assert!(o.allow_write);
        assert_eq!(o.files, [PathBuf::from("a.csv")]);
        assert!(Options::parse(["--allow-dir".to_string()]).is_err());
        assert!(Options::parse(["--nope".to_string()]).is_err());
        assert!(Options::parse(["--help".to_string()]).unwrap_err().contains("Usage"));
    }
}

#[cfg(test)]
mod tool_tests {
    use super::*;

    fn text(r: &CallToolResult) -> String {
        serde_json::to_value(&r.content[0]).unwrap()["text"].as_str().unwrap_or_default().to_string()
    }

    fn server(write: bool) -> (tempfile::TempDir, ParvuMcp) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("sales.csv"), "id,region,qty\n1,north,3\n2,south,5\n3,north,7\n").unwrap();
        let opts = Options { allow_dirs: vec![dir.path().to_path_buf()], allow_write: write, ..Default::default() };
        let s = ParvuMcp::new(opts).unwrap();
        (dir, s)
    }

    #[tokio::test]
    async fn open_query_apply_undo() {
        let (dir, s) = server(false);
        let p = dir.path().join("sales.csv").display().to_string();
        let r = s.open_file(Parameters(OpenFileArgs { path: p, sheet: None })).await.unwrap();
        assert_ne!(r.is_error, Some(true), "{}", text(&r));
        assert!(text(&r).contains("\"table\":\"sales\""));

        let q = |sql: &str| QueryArgs { sql: sql.into(), max_rows: 100 };
        let r = s.query(Parameters(q("SELECT SUM(qty) AS t FROM sales WHERE region = 'north'"))).await.unwrap();
        assert!(text(&r).contains("[[\"10\"]]"), "{}", text(&r));

        let op: Operation = serde_json::from_value(json!({"op": "remove", "column": "qty"})).unwrap();
        let r = s.apply_impl(ApplyArgs { table: "sales".into(), operation: op, rationale: None }, "test".into()).await.unwrap();
        assert!(!text(&r).contains("\"name\":\"qty\""), "{}", text(&r));
        let r = s.undo_impl(TableArgs { table: "sales".into() }, "test".into()).await.unwrap();
        assert!(text(&r).contains("\"step\":\"step.remove\""), "{}", text(&r));
        let r = s.describe_table(Parameters(TableArgs { table: "sales".into() })).await.unwrap();
        assert!(text(&r).contains("\"name\":\"qty\""), "undo restored the column: {}", text(&r));

        // Mistakes come back as tool errors the model can read.
        let r = s.query(Parameters(q("SELECT * FROM read_csv('/etc/passwd')"))).await.unwrap();
        assert_eq!(r.is_error, Some(true));
        let r = s.describe_table(Parameters(TableArgs { table: "nope".into() })).await.unwrap();
        assert_eq!(r.is_error, Some(true));
    }

    #[tokio::test]
    async fn headless_formulas() {
        let (dir, s) = server(false);
        let p = dir.path().join("sales.csv").display().to_string();
        s.open_file(Parameters(OpenFileArgs { path: p, sheet: None })).await.unwrap();
        let args = SetFormulaArgs {
            table: "sales".into(),
            name: "double".into(),
            formula: "= [qty] * 2".into(),
            after: Some("id".into()),
            rationale: None,
        };
        let r = s.formula_impl(args, "test".into()).await.unwrap();
        assert_ne!(r.is_error, Some(true), "{}", text(&r));
        assert!(text(&r).contains("[\"1\",\"6\",\"north\",\"3\"]"), "{}", text(&r));
    }

    /// Custom functions and operations through MCP: listed with how to call
    /// them, and applied with apply_operation and set_formula.
    #[tokio::test]
    async fn custom_tools_are_listed_and_applied() {
        let (dir, s) = server(false);
        s.workspace().unwrap().lock().unwrap().set_custom(crate::custom::test_support::sample_library());
        std::fs::write(dir.path().join("people.csv"), "name,phone,score\nann,+1 (555) 010-2030,3\nbob,n/a,9\ncid,555.0102,5\n").unwrap();
        let p = dir.path().join("people.csv").display().to_string();
        s.open_file(Parameters(OpenFileArgs { path: p, sheet: None })).await.unwrap();

        let r = s.list_custom_tools().await.unwrap();
        let list: serde_json::Value = serde_json::from_str(&text(&r)).unwrap();
        let tools = list["custom_tools"].as_array().unwrap();
        let find = |n: &str| tools.iter().find(|t| t["name"] == n).unwrap_or_else(|| panic!("{n} not listed")).clone();
        let phone = find("clean_phone");
        assert_eq!(phone["kind"], "row function");
        assert!(phone["signature"].as_str().unwrap().starts_with("clean_phone(phone: column<text>"));
        assert_eq!(phone["use"]["apply_operation"]["op"], "custom_function");
        assert_eq!(phone["use"]["query"], "SELECT clean_phone(<phone column>) FROM <table>");
        assert_eq!(find("weighted_average")["kind"], "summary function");
        assert_eq!(find("top_n_per_group")["use"]["apply_operation"]["op"], "custom_operation");

        // A custom function, as the listing says to call it.
        let op: Operation = serde_json::from_value(json!({
            "op": "custom_function", "name": "clean_phone",
            "args": {"phone": {"column": "phone"}}, "output": {"to": "new", "name": "phone_clean"}
        }))
        .unwrap();
        let r = s.apply_impl(ApplyArgs { table: "people".into(), operation: op, rationale: None }, "test".into()).await.unwrap();
        assert_ne!(r.is_error, Some(true), "{}", text(&r));
        assert!(text(&r).contains("\"+15550102030\""), "{}", text(&r));
        // A custom operation.
        let op: Operation = serde_json::from_value(json!({
            "op": "custom_operation", "name": "top_n_per_group",
            "args": {"grp": {"column": "name"}, "score": {"column": "score"}, "n": {"value": "1"}}
        }))
        .unwrap();
        let r = s.apply_impl(ApplyArgs { table: "people".into(), operation: op, rationale: None }, "test".into()).await.unwrap();
        assert_ne!(r.is_error, Some(true), "{}", text(&r));
        assert!(text(&r).contains("\"rows\":3"), "{}", text(&r));
        // A summary function in a formula is a total.
        let args = SetFormulaArgs {
            table: "people".into(),
            name: "vs_mean".into(),
            formula: "= [score] - weighted_average([score], [score])".into(),
            after: None,
            rationale: None,
        };
        let r = s.formula_impl(args, "test".into()).await.unwrap();
        assert_ne!(r.is_error, Some(true), "{}", text(&r));
        // A name that isn't defined is an error the model can read.
        let op: Operation = serde_json::from_value(json!({"op": "custom_function", "name": "nope", "args": {}})).unwrap();
        let r = s.apply_impl(ApplyArgs { table: "people".into(), operation: op, rationale: None }, "test".into()).await.unwrap();
        assert_eq!(r.is_error, Some(true));
    }

    /// The query tool expands custom function calls before the read-only
    /// guard checks the SQL, so a file function inside an argument is still
    /// refused.
    #[tokio::test]
    async fn query_calls_custom_functions() {
        let (dir, s) = server(false);
        s.workspace().unwrap().lock().unwrap().set_custom(crate::custom::test_support::sample_library());
        std::fs::write(dir.path().join("people.csv"), "name,phone,score\nann,+1 (555) 010-2030,3\nbob,n/a,9\ncid,555.0102,5\n").unwrap();
        let p = dir.path().join("people.csv").display().to_string();
        s.open_file(Parameters(OpenFileArgs { path: p, sheet: None })).await.unwrap();

        let sql = "SELECT name, clean_phone(phone) AS p, weighted_average(score, 1) OVER () AS wa FROM people ORDER BY name";
        let q = |sql: &str| QueryArgs { sql: sql.into(), max_rows: 10 };
        let r = s.query(Parameters(q(sql))).await.unwrap();
        assert_ne!(r.is_error, Some(true), "{}", text(&r));
        assert!(text(&r).contains("[\"ann\",\"+15550102030\",\"5.66666666666667\"]"), "{}", text(&r));

        let r = s.query(Parameters(q("SELECT clean_phone((SELECT content FROM read_text('/etc/passwd'))) FROM people"))).await.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(text(&r).contains("read_text"), "{}", text(&r));
        let r = s.query(Parameters(q("SELECT clean_phone(phone, '+', 1) FROM people"))).await.unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(text(&r).contains("clean_phone takes 1 to 2 arguments"), "{}", text(&r));
    }

    pub(super) fn shout(body: &str) -> serde_json::Value {
        json!({
            "kind": "function", "name": "shout",
            "params": [{"name": "s", "kind": "column", "type": ["text"]}],
            "result": {"kind": "column", "type": ["text"]},
            "body": {"language": "sql", "source": body},
            "examples": [{
                "input": {"columns": [{"name": "s", "dtype": "VARCHAR"}], "rows": [["a"], [null]]},
                "expected": {"columns": [{"name": "result"}], "rows": [["A!"], [null]]}
            }]
        })
    }

    /// Headless, a proposal is only validated: the report comes back and
    /// nothing is written. File functions are refused by the guard.
    #[tokio::test]
    async fn headless_proposals_are_only_validated() {
        let (dir, s) = server(true);
        let propose = |definition: serde_json::Value| ProposeCustomArgs { definition, replaces: None, rationale: None, offer: None };
        let r = s.propose_impl(propose(shout("upper(s) || '!'")), "test".into()).await.unwrap();
        assert_ne!(r.is_error, Some(true), "{}", text(&r));
        let v: serde_json::Value = serde_json::from_str(&text(&r)).unwrap();
        assert_eq!(v["passed"], true, "{v}");
        assert!(v["next"].as_str().unwrap().contains("never saves"));
        assert!(v["compiled_template"].as_str().unwrap().contains("__pv_arg_0"));
        // A failing example is reported with what the body gave.
        let r = s.propose_impl(propose(shout("upper(s)")), "test".into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&text(&r)).unwrap();
        assert_eq!(v["passed"], false);
        assert_eq!(v["examples"][0]["passed"], false);
        assert!(v["examples"][0]["actual"].is_object(), "{v}");
        // The read-only guard refuses a body that reads files.
        let r = s.propose_impl(propose(shout("(SELECT content FROM read_text('/etc/passwd')) || s")), "test".into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&text(&r)).unwrap();
        assert_eq!(v["passed"], false);
        let safety = v["checks"].as_array().unwrap().iter().find(|c| c["check"] == "safety").unwrap_or_else(|| panic!("{v}")).clone();
        assert_eq!(safety["status"], "fail", "{v}");
        // Not a definition at all.
        let r = s.propose_impl(propose(json!({"name": 3})), "test".into()).await.unwrap();
        assert_eq!(r.is_error, Some(true));
        // Nothing was written, even with --allow-write.
        let files: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(files.len(), 1, "{files:?}");
    }

    #[tokio::test]
    async fn files_stay_inside_allowed_folders() {
        let (dir, s) = server(false);
        let outside = tempfile::tempdir().unwrap();
        let f = outside.path().join("x.csv");
        std::fs::write(&f, "a\n1\n").unwrap();
        let r = s.open_file(Parameters(OpenFileArgs { path: f.display().to_string(), sheet: None })).await.unwrap();
        assert_eq!(r.is_error, Some(true));
        // `..` does not escape after canonicalisation.
        let sneaky = dir.path().join("..").join(outside.path().file_name().unwrap()).join("x.csv");
        let r = s.open_file(Parameters(OpenFileArgs { path: sneaky.display().to_string(), sheet: None })).await.unwrap();
        assert_eq!(r.is_error, Some(true));

        let export = |path: PathBuf| ExportArgs { table: "sales".into(), path: path.display().to_string(), overwrite: false };
        let p = dir.path().join("sales.csv").display().to_string();
        s.open_file(Parameters(OpenFileArgs { path: p, sheet: None })).await.unwrap();
        let r = s.export(Parameters(export(dir.path().join("out.csv")))).await.unwrap();
        assert_eq!(r.is_error, Some(true), "export needs --allow-write");

        let (dir, s) = server(true);
        let p = dir.path().join("sales.csv").display().to_string();
        s.open_file(Parameters(OpenFileArgs { path: p, sheet: None })).await.unwrap();
        let r = s.export(Parameters(export(dir.path().join("out.parquet")))).await.unwrap();
        assert_ne!(r.is_error, Some(true), "{}", text(&r));
        assert!(dir.path().join("out.parquet").is_file());
        let r = s.export(Parameters(export(dir.path().join("out.md")))).await.unwrap();
        assert_ne!(r.is_error, Some(true), "{}", text(&r));
        let md = std::fs::read_to_string(dir.path().join("out.md")).unwrap();
        assert!(md.starts_with("| ") && md.lines().nth(1).unwrap().contains("---"), "{md}");
        let r = s.export(Parameters(export(dir.path().join("out.parquet")))).await.unwrap();
        assert_eq!(r.is_error, Some(true), "no silent overwrite");
        let r = s.export(Parameters(export(outside.path().join("out.csv")))).await.unwrap();
        assert_eq!(r.is_error, Some(true), "no writes outside");
        assert!(!outside.path().join("out.csv").exists());
    }

    #[test]
    fn no_folders_means_no_files() {
        let s = ParvuMcp::new(Options::default()).unwrap();
        assert!(s.allowed(Path::new("/tmp")).unwrap_err().contains("--allow-dir"));
        assert!(s.instructions().contains("disabled"));
    }
}

#[cfg(test)]
mod attach_tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tokio::io::AsyncBufReadExt;

    /// A host with one workspace that answers every proposal with `answer`.
    struct FakeHost {
        ws: Arc<Mutex<Workspace>>,
        answer: Decision,
        seen: Mutex<Vec<Proposal>>,
        changed: Mutex<Vec<u64>>,
        offers: Mutex<Vec<CustomOffer>>,
    }

    impl AttachHost for FakeHost {
        fn workspace(&self) -> Result<(String, Arc<Mutex<Workspace>>), String> {
            Ok(("main".into(), self.ws.clone()))
        }
        fn propose(&self, p: Proposal) -> Pin<Box<dyn Future<Output = Decision> + Send>> {
            self.seen.lock().unwrap().push(p);
            let d = self.answer;
            Box::pin(async move { d })
        }
        fn changed(&self, _window: &str, tab_id: u64) {
            self.changed.lock().unwrap().push(tab_id);
        }
        fn offer_custom(&self, _window: &str, offer: CustomOffer) {
            self.offers.lock().unwrap().push(offer);
        }
    }

    fn host(answer: Decision) -> (tempfile::TempDir, Arc<FakeHost>) {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("sales.csv");
        std::fs::write(&f, "id,region,qty\n1,north,3\n2,south,5\n3,north,7\n").unwrap();
        let mut ws = Workspace::new(100, None).unwrap();
        ws.open(&f).unwrap();
        let h = FakeHost {
            ws: Arc::new(Mutex::new(ws)),
            answer,
            seen: Mutex::default(),
            changed: Mutex::default(),
            offers: Mutex::default(),
        };
        (dir, Arc::new(h))
    }

    /// A line-based MCP client over TCP.
    struct Client {
        read: BufReader<tokio::net::tcp::OwnedReadHalf>,
        write: tokio::net::tcp::OwnedWriteHalf,
        next: u64,
    }

    impl Client {
        async fn connect(port: u16, token: &str) -> Self {
            let s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            let (r, mut w) = s.into_split();
            w.write_all(format!("{token}\n").as_bytes()).await.unwrap();
            let mut c = Client { read: BufReader::new(r), write: w, next: 0 };
            let init = c
                .call("initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "Test Client", "version": "1"}}))
                .await;
            assert!(init["result"]["instructions"].as_str().unwrap().contains("running ParVu window"));
            c.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).await;
            c
        }
        async fn send(&mut self, v: serde_json::Value) {
            self.write.write_all(format!("{v}\n").as_bytes()).await.unwrap();
        }
        async fn call(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
            self.next += 1;
            let id = self.next;
            self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).await;
            loop {
                let mut line = String::new();
                let n = tokio::time::timeout(Duration::from_secs(10), self.read.read_line(&mut line)).await.unwrap().unwrap();
                assert!(n > 0, "server closed the connection");
                let v: serde_json::Value = serde_json::from_str(&line).unwrap();
                if v["id"] == json!(id) {
                    return v;
                }
            }
        }
        async fn tool(&mut self, name: &str, args: serde_json::Value) -> (bool, String) {
            let r = self.call("tools/call", json!({"name": name, "arguments": args})).await;
            let res = &r["result"];
            (res["isError"] == json!(true), res["content"][0]["text"].as_str().unwrap_or_default().to_string())
        }
    }

    async fn listen(h: Arc<FakeHost>) -> (u16, String) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let token = new_token();
        let t = token.clone();
        tokio::spawn(serve_tcp(listener, t, move || ParvuMcp::attached(h.clone()), |_| {}));
        (port, token)
    }

    #[tokio::test]
    async fn approved_changes_apply_and_rejected_ones_dont() {
        let (_d, h) = host(Decision::Approved);
        let (port, token) = listen(h.clone()).await;
        let mut c = Client::connect(port, &token).await;

        let tools = c.call("tools/list", json!({})).await;
        let names: Vec<_> = tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
        assert!(names.contains(&"query".to_string()) && names.contains(&"apply_operation".to_string()));
        assert!(!names.contains(&"open_file".to_string()) && !names.contains(&"export".to_string()), "{names:?}");

        let (err, text) = c.tool("query", json!({"sql": "SELECT SUM(qty) AS t FROM sales"})).await;
        assert!(!err && text.contains("[[\"15\"]]"), "{text}");

        let op = json!({"op": "filter", "conditions": [{"column": "region", "op": "eq", "value": "north"}]});
        let (err, text) = c.tool("apply_operation", json!({"table": "sales", "operation": op, "rationale": "Only the north region"})).await;
        assert!(!err && text.contains("\"rows\":2"), "{text}");
        {
            let seen = h.seen.lock().unwrap();
            let p = &seen[0];
            assert_eq!((p.client.as_str(), p.table.as_str(), p.kind), ("Test Client", "sales", "operation"));
            assert_eq!((p.rows_before, p.rows_after), (3, Some(2)));
            assert_eq!(p.rationale.as_deref(), Some("Only the north region"));
            assert_eq!(p.label.key, "step.filter_rows");
            assert!(!p.rows.is_empty() && p.sql.is_some());
        }
        assert_eq!(h.changed.lock().unwrap().len(), 1, "the window is told to refresh");
        assert_eq!(h.ws.lock().unwrap().summary(1).unwrap().total_rows, 2);

        let (err, text) = c.tool("undo", json!({"table": "sales"})).await;
        assert!(!err && text.contains("\"rows\":3"), "{text}");
        assert_eq!(h.seen.lock().unwrap()[1].kind, "undo");

        // A file read hidden in an expression is refused before any card.
        let sneaky = json!({"op": "add_column", "name": "x", "expression": "(SELECT 1 FROM read_text('/etc/passwd'))"});
        let (err, _) = c.tool("apply_operation", json!({"table": "sales", "operation": sneaky})).await;
        assert!(err);
        assert_eq!(h.seen.lock().unwrap().len(), 2);

        let (_d2, h2) = host(Decision::Rejected);
        let (port, token) = listen(h2.clone()).await;
        let mut c = Client::connect(port, &token).await;
        let op = json!({"op": "remove", "column": "qty"});
        let (err, text) = c.tool("apply_operation", json!({"table": "sales", "operation": op})).await;
        assert!(err && text.contains("rejected"), "{text}");
        assert!(h2.ws.lock().unwrap().summary(1).unwrap().steps.is_empty(), "nothing applied");
        assert!(h2.changed.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn table_operations_open_a_new_table() {
        let (_d, h) = host(Decision::Approved);
        let (port, token) = listen(h.clone()).await;
        let mut c = Client::connect(port, &token).await;
        let op = json!({"op": "group_by", "by": ["region"], "aggregations": [{"column": "qty", "func": "sum"}]});
        let (err, text) = c.tool("apply_operation", json!({"table": "sales", "operation": op, "rationale": "Totals"})).await;
        assert!(!err && text.contains("\"new_table\":\"sales_by_region\"") && text.contains("\"rows\":2"), "{text}");
        assert_eq!(h.seen.lock().unwrap()[0].kind, "new_table");
        {
            let ws = h.ws.lock().unwrap();
            let new = ws.tab_ids()[1];
            assert_eq!(*h.changed.lock().unwrap(), vec![new], "the window is told about the new tab");
            assert_eq!(ws.summary(new).unwrap().origin.unwrap().source, "sales");
            assert!(ws.summary(1).unwrap().steps.is_empty(), "the source is unchanged");
        }
        let (err, text) = c.tool("query", json!({"sql": "SELECT count(*) FROM sales_by_region"})).await;
        assert!(!err && text.contains("[[\"2\"]]"), "{text}");
    }

    #[tokio::test]
    async fn formulas_are_proposed_with_a_preview() {
        let (_d, h) = host(Decision::Approved);
        let (port, token) = listen(h.clone()).await;
        let mut c = Client::connect(port, &token).await;
        let args = json!({"table": "sales", "name": "share", "formula": "[qty] / SUM([qty])", "rationale": "Each row's share"});
        let (err, text) = c.tool("set_formula", args).await;
        assert!(!err && text.contains("\"step\":\"step.formula_add\""), "{text}");
        {
            let seen = h.seen.lock().unwrap();
            let p = &seen[0];
            assert_eq!((p.kind, p.label.key.as_str()), ("formula", "step.formula_add"));
            assert_eq!(p.sql.as_deref(), Some("= [qty] / SUM([qty])"));
            assert_eq!(p.columns.last().unwrap().name, "share", "the preview shows the new column");
            assert_eq!(p.rows[0].last().unwrap().as_deref(), Some("0.2"));
        }
        let source = h.ws.lock().unwrap().summary(1).unwrap().formulas[0].source.clone();
        assert_eq!(source, "[qty] / SUM([qty])");
        // A formula that doesn't compile is an error before any card.
        let (err, text) = c.tool("set_formula", json!({"table": "sales", "name": "x", "formula": "[nope] + 1"})).await;
        assert!(err && text.contains("nope"), "{text}");
        assert_eq!(h.seen.lock().unwrap().len(), 1);

        let (_d2, h2) = host(Decision::Rejected);
        let (port, token) = listen(h2.clone()).await;
        let mut c = Client::connect(port, &token).await;
        let (err, text) = c.tool("set_formula", json!({"table": "sales", "name": "d", "formula": "[qty] * 2"})).await;
        assert!(err && text.contains("rejected"), "{text}");
        assert!(h2.ws.lock().unwrap().summary(1).unwrap().formulas.is_empty());
    }

    /// In the window, a proposal is validated and offered to the user in the
    /// editor (passing or not); it is never saved by the server.
    #[tokio::test]
    async fn attached_proposals_are_offered_not_saved() {
        let (_d, h) = host(Decision::Approved);
        h.ws.lock().unwrap().set_custom(crate::custom::test_support::sample_library());
        let (port, token) = listen(h.clone()).await;
        let mut c = Client::connect(port, &token).await;
        let (err, text) = c.tool("list_custom_tools", json!({})).await;
        assert!(!err && text.contains("\"name\":\"clean_phone\""), "{text}");
        let (err, text) = c.tool("propose_custom_tool", json!({"definition": super::tool_tests::shout("upper(s) || '!'"), "rationale": "Shouting"})).await;
        assert!(!err && text.contains("\"passed\":true") && text.contains("offered to the user"), "{text}");
        let (err, text) = c.tool("propose_custom_tool", json!({"definition": super::tool_tests::shout("upper(s)"), "offer": false})).await;
        assert!(!err && text.contains("\"passed\":false") && text.contains("Not offered"), "{text}");
        let offers = h.offers.lock().unwrap();
        assert_eq!(offers.len(), 1, "offer: false offers nothing");
        assert_eq!((offers[0].definition.name.as_str(), offers[0].passed), ("shout", true));
        assert_eq!(offers[0].rationale.as_deref(), Some("Shouting"));
        assert!(offers[0].definition.report.is_some(), "the editor opens with the report");
        assert!(h.seen.lock().unwrap().is_empty(), "a proposal is not an approval card");
    }

    #[tokio::test]
    async fn wrong_token_is_refused() {
        let (_d, h) = host(Decision::Approved);
        let (port, _token) = listen(h).await;
        let s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let (r, mut w) = s.into_split();
        w.write_all(b"not-the-token\n").await.unwrap();
        w.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n").await.unwrap();
        let mut line = String::new();
        let n = tokio::time::timeout(Duration::from_secs(5), BufReader::new(r).read_line(&mut line)).await.unwrap().unwrap_or(0);
        assert_eq!(n, 0, "connection closed without an answer: {line}");
    }

    #[test]
    fn run_file_is_private_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("run").join("mcp.json");
        let info = RunInfo { port: 4242, token: new_token(), pid: 7 };
        write_run_file(&p, &info).unwrap();
        write_run_file(&p, &info).unwrap();
        let back: RunInfo = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(back, info);
        assert_eq!(info.token.len(), 64);
        assert_ne!(new_token(), new_token());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        }
        assert!(same_token("abc", "abc") && !same_token("abc", "abd") && !same_token("abc", "ab"));
        assert_eq!(display_name(&Implementation::new("claude-code", "2")), "Claude Code");
        let mut titled = Implementation::new("x", "1");
        titled.title = Some("Claude Desktop".into());
        assert_eq!(display_name(&titled), "Claude Desktop");
        assert!(Options::parse(["--attach".to_string()]).unwrap().attach);
        assert!(Options::parse(["--attach", "--allow-write"].map(String::from)).is_err());
    }
}
