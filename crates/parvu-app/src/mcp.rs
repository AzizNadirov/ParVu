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

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use parvu_core::Workspace;
use parvu_core::export::{ExportFormat, ExportOptions};
use parvu_core::ops::Operation;
use parvu_core::page::{Cell, ColumnMeta};
use parvu_core::readonly::{QueryRows, ReadSession, SampleRows};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ErrorData, ServerHandler, ServiceExt, schemars, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};
use serde_json::json;

const QUERY_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_QUERY_ROWS: usize = 1000;
const MAX_SAMPLE_ROWS: usize = 100;

const USAGE: &str = "\
Usage: parvu mcp [--allow-dir DIR]... [--allow-write] [FILE]...

Runs ParVu as an MCP server on stdin/stdout for AI clients.
  --allow-dir DIR   folder the client may open files from (repeatable)
  --allow-write     allow the export tool to write files into those folders
  FILE              files to open at start (always allowed)";

/// Command-line options of `parvu mcp`.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Options {
    pub allow_dirs: Vec<PathBuf>,
    pub allow_write: bool,
    pub files: Vec<PathBuf>,
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
                "-h" | "--help" => return Err(USAGE.into()),
                s if s.starts_with("--") => return Err(format!("Unknown option {s}\n\n{USAGE}")),
                file => o.files.push(expand_home(file)),
            }
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
    runtime.block_on(async move {
        let server = match ParvuMcp::new(options) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("parvu mcp: {e}");
                return 1;
            }
        };
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

/// The server: one workspace for the whole session.
#[derive(Clone)]
pub struct ParvuMcp {
    ws: Arc<Mutex<Workspace>>,
    allow_dirs: Arc<Vec<PathBuf>>,
    allow_write: bool,
    tool_router: ToolRouter<Self>,
}

// ---------- tool parameters ----------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct OpenFileArgs {
    /// Path of a Parquet, CSV, TSV, JSON / JSON Lines or Excel file inside an allowed folder.
    pub path: String,
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
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExportArgs {
    /// Table to write.
    pub table: String,
    /// Output path inside an allowed folder; the extension picks the format
    /// (.csv, .parquet, .json, .jsonl, .xlsx).
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
        for f in &options.files {
            ws.open(f).map_err(|e| format!("{}: {e}", f.display()))?;
        }
        Ok(Self {
            ws: Arc::new(Mutex::new(ws)),
            allow_dirs: Arc::new(allow_dirs),
            allow_write: options.allow_write,
            tool_router: Self::tool_router(),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Workspace> {
        self.ws.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// A read-only snapshot, taken under the lock and used without it.
    fn session(&self) -> parvu_core::Result<ReadSession> {
        self.lock().read_session()
    }

    /// Run blocking DuckDB work off the async runtime.
    async fn blocking<T: Send + 'static>(
        f: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        tokio::task::spawn_blocking(f).await.map_err(|e| e.to_string())?
    }

    /// `path` canonicalised, if it lies inside an allowed folder.
    fn allowed(&self, path: &Path) -> Result<PathBuf, String> {
        if self.allow_dirs.is_empty() {
            return Err("No folders are allowed. Start the server with --allow-dir <folder>.".into());
        }
        let canon = path.canonicalize().map_err(|e| format!("{}: {e}", path.display()))?;
        if self.allow_dirs.iter().any(|d| canon.starts_with(d)) {
            Ok(canon)
        } else {
            Err(format!("{} is outside the allowed folders: {}", path.display(), self.allowed_list()))
        }
    }

    fn allowed_list(&self) -> String {
        self.allow_dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join(", ")
    }

    fn tab_id(ws: &Workspace, table: &str) -> Result<u64, String> {
        ws.tab_ids()
            .into_iter()
            .find(|&id| ws.tab_name(id).is_ok_and(|n| n.eq_ignore_ascii_case(table)))
            .ok_or_else(|| format!("Unknown table \"{table}\"."))
    }

    fn instructions(&self) -> String {
        let dirs = if self.allow_dirs.is_empty() { "none (opening files is disabled)".into() } else { self.allowed_list() };
        format!(
            "ParVu opens Parquet, CSV, JSON and Excel files as tables and queries them with DuckDB.\n\
             Start with list_tables, or open_file for a file in the allowed folders ({dirs}).\n\
             Inspect before answering: describe_table, sample_rows, value_counts, column_stats.\n\
             query runs one DuckDB SELECT over the open tables by name; file functions such as \
             read_csv are refused. Quote identifiers with spaces in double quotes.\n\
             apply_operation changes a table for this session only (filters, group by, pivot, \
             new columns…); undo reverts it. Files on disk are never changed{}.",
            if self.allow_write { ", except by export" } else { "" }
        )
    }
}

#[tool_router]
impl ParvuMcp {
    #[tool(description = "List the open tables with their row and column counts.")]
    async fn list_tables(&self) -> Result<CallToolResult, ErrorData> {
        let ro = try_tool!(self.session());
        let out = try_tool!(
            Self::blocking(move || {
                ro.tables()
                    .iter()
                    .map(|t| {
                        ro.describe_table(t)
                            .map(|i| json!({ "table": i.name, "rows": i.rows, "columns": i.columns.len(), "steps": i.steps.len() }))
                    })
                    .collect::<parvu_core::Result<Vec<_>>>()
                    .map_err(|e| e.to_string())
            })
            .await
        );
        ok(json!({ "tables": out }))
    }

    #[tool(description = "Open a data file (Parquet, CSV, TSV, JSON, JSON Lines, Excel) from an allowed folder as a table. Returns the table name and columns.")]
    async fn open_file(&self, Parameters(args): Parameters<OpenFileArgs>) -> Result<CallToolResult, ErrorData> {
        let path = try_tool!(self.allowed(&expand_home(&args.path)));
        let ws = self.ws.clone();
        let out = try_tool!(
            Self::blocking(move || {
                let mut ws = ws.lock().unwrap_or_else(|p| p.into_inner());
                let id = ws.open(&path).map_err(|e| e.to_string())?;
                let s = ws.summary(id).map_err(|e| e.to_string())?;
                let cols = ws.columns(id).map_err(|e| e.to_string())?;
                Ok(json!({ "table": s.name, "rows": s.total_rows, "columns": columns_json(&cols) }))
            })
            .await
        );
        ok(out)
    }

    #[tool(description = "Columns with types, row count and applied steps of a table.")]
    async fn describe_table(&self, Parameters(args): Parameters<TableArgs>) -> Result<CallToolResult, ErrorData> {
        let ro = try_tool!(self.session());
        let info = try_tool!(Self::blocking(move || ro.describe_table(&args.table).map_err(|e| e.to_string())).await);
        let steps: Vec<_> = info.steps.iter().map(|s| json!({ "step": s.key, "args": s.args })).collect();
        ok(json!({ "table": info.name, "rows": info.rows, "columns": columns_json(&info.columns), "steps": steps }))
    }

    #[tool(description = "First, last or random rows of a table.")]
    async fn sample_rows(&self, Parameters(args): Parameters<SampleArgs>) -> Result<CallToolResult, ErrorData> {
        let ro = try_tool!(self.session());
        let n = args.n.clamp(1, MAX_SAMPLE_ROWS);
        let rows = try_tool!(Self::blocking(move || ro.sample(&args.table, args.mode, n).map_err(|e| e.to_string())).await);
        ok(rows_json(&rows))
    }

    #[tool(description = "Statistics for one column: rows, nulls, distinct values, min, max, mean and standard deviation.")]
    async fn column_stats(&self, Parameters(args): Parameters<ColumnArgs>) -> Result<CallToolResult, ErrorData> {
        let ro = try_tool!(self.session());
        let stats =
            try_tool!(Self::blocking(move || ro.column_stats(&args.table, &args.column).map_err(|e| e.to_string())).await);
        ok(serde_json::to_value(stats).unwrap_or_default())
    }

    #[tool(description = "The most frequent values of a column with their counts.")]
    async fn value_counts(&self, Parameters(args): Parameters<ValueCountsArgs>) -> Result<CallToolResult, ErrorData> {
        let ro = try_tool!(self.session());
        let k = args.k.clamp(1, MAX_SAMPLE_ROWS);
        let rows =
            try_tool!(Self::blocking(move || ro.value_counts(&args.table, &args.column, k).map_err(|e| e.to_string())).await);
        ok(rows_json(&rows))
    }

    #[tool(description = "Run one read-only DuckDB SELECT over the open tables (referenced by table name) and return the rows. Joins, CTEs, window functions and aggregates work; reading other files does not.")]
    async fn query(&self, Parameters(args): Parameters<QueryArgs>) -> Result<CallToolResult, ErrorData> {
        let ro = try_tool!(self.session());
        let max = args.max_rows.clamp(1, MAX_QUERY_ROWS);
        let rows =
            try_tool!(Self::blocking(move || ro.query(&args.sql, max, Some(QUERY_TIMEOUT)).map_err(|e| e.to_string())).await);
        ok(rows_json(&rows))
    }

    #[tool(description = "Apply a table operation (filter, sort, group by, pivot, melt, add column, fill missing values, window functions, …) as a new step on a table. Changes only this session's table, never the file. Returns the new row count and the first rows.")]
    async fn apply_operation(&self, Parameters(args): Parameters<ApplyArgs>) -> Result<CallToolResult, ErrorData> {
        let ro = try_tool!(self.session());
        let ws = self.ws.clone();
        let out = try_tool!(
            Self::blocking(move || {
                ro.check_operation(&args.table, &args.operation).map_err(|e| e.to_string())?;
                drop(ro);
                let mut ws = ws.lock().unwrap_or_else(|p| p.into_inner());
                let id = Self::tab_id(&ws, &args.table)?;
                let label = ws.apply(id, &args.operation).map_err(|e| e.to_string())?;
                let s = ws.summary(id).map_err(|e| e.to_string())?;
                let page = ws.page(id, 1).map_err(|e| e.to_string())?;
                let first: Vec<_> = page.rows.into_iter().take(10).collect();
                Ok(json!({
                    "table": s.name,
                    "step": { "step": label.key, "args": label.args },
                    "rows": s.total_rows,
                    "columns": columns_json(&page.columns),
                    "first_rows": first,
                }))
            })
            .await
        );
        ok(out)
    }

    #[tool(description = "Undo the last applied operation on a table.")]
    async fn undo(&self, Parameters(args): Parameters<TableArgs>) -> Result<CallToolResult, ErrorData> {
        let ws = self.ws.clone();
        let out = try_tool!(
            Self::blocking(move || {
                let mut ws = ws.lock().unwrap_or_else(|p| p.into_inner());
                let id = Self::tab_id(&ws, &args.table)?;
                let label = ws.undo(id).map_err(|e| e.to_string())?;
                let s = ws.summary(id).map_err(|e| e.to_string())?;
                Ok(json!({ "table": s.name, "undone": { "step": label.key, "args": label.args }, "rows": s.total_rows }))
            })
            .await
        );
        ok(out)
    }

    #[tool(description = "Write a table (with its applied steps) to a file in an allowed folder. Only available when the server runs with --allow-write.")]
    async fn export(&self, Parameters(args): Parameters<ExportArgs>) -> Result<CallToolResult, ErrorData> {
        if !self.allow_write {
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
        let ws = self.ws.clone();
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
        let opts = Options { allow_dirs: vec![dir.path().to_path_buf()], allow_write: write, files: vec![] };
        let s = ParvuMcp::new(opts).unwrap();
        (dir, s)
    }

    #[tokio::test]
    async fn open_query_apply_undo() {
        let (dir, s) = server(false);
        let p = dir.path().join("sales.csv").display().to_string();
        let r = s.open_file(Parameters(OpenFileArgs { path: p })).await.unwrap();
        assert_ne!(r.is_error, Some(true), "{}", text(&r));
        assert!(text(&r).contains("\"table\":\"sales\""));

        let q = |sql: &str| QueryArgs { sql: sql.into(), max_rows: 100 };
        let r = s.query(Parameters(q("SELECT SUM(qty) AS t FROM sales WHERE region = 'north'"))).await.unwrap();
        assert!(text(&r).contains("[[\"10\"]]"), "{}", text(&r));

        let op: Operation = serde_json::from_value(json!({"op": "remove", "column": "qty"})).unwrap();
        let r = s.apply_operation(Parameters(ApplyArgs { table: "sales".into(), operation: op })).await.unwrap();
        assert!(!text(&r).contains("\"name\":\"qty\""), "{}", text(&r));
        let r = s.undo(Parameters(TableArgs { table: "sales".into() })).await.unwrap();
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
    async fn files_stay_inside_allowed_folders() {
        let (dir, s) = server(false);
        let outside = tempfile::tempdir().unwrap();
        let f = outside.path().join("x.csv");
        std::fs::write(&f, "a\n1\n").unwrap();
        let r = s.open_file(Parameters(OpenFileArgs { path: f.display().to_string() })).await.unwrap();
        assert_eq!(r.is_error, Some(true));
        // `..` does not escape after canonicalisation.
        let sneaky = dir.path().join("..").join(outside.path().file_name().unwrap()).join("x.csv");
        let r = s.open_file(Parameters(OpenFileArgs { path: sneaky.display().to_string() })).await.unwrap();
        assert_eq!(r.is_error, Some(true));

        let export = |path: PathBuf| ExportArgs { table: "sales".into(), path: path.display().to_string(), overwrite: false };
        let p = dir.path().join("sales.csv").display().to_string();
        s.open_file(Parameters(OpenFileArgs { path: p })).await.unwrap();
        let r = s.export(Parameters(export(dir.path().join("out.csv")))).await.unwrap();
        assert_eq!(r.is_error, Some(true), "export needs --allow-write");

        let (dir, s) = server(true);
        let p = dir.path().join("sales.csv").display().to_string();
        s.open_file(Parameters(OpenFileArgs { path: p })).await.unwrap();
        let r = s.export(Parameters(export(dir.path().join("out.parquet")))).await.unwrap();
        assert_ne!(r.is_error, Some(true), "{}", text(&r));
        assert!(dir.path().join("out.parquet").is_file());
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
