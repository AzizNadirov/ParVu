//! All open tabs of one window, sharing one DuckDB connection.
//!
//! Each tab is registered under three names:
//! - `__pv_src_<name>`: a view reading the file (or, for Excel, the table
//!   `__pv_xlsx_<name>` loaded from it);
//! - `<name>`: the view users query. It equals the source view, or the source
//!   with pending cell edits overlaid (see [`crate::edits::overlay_sql`]);
//! - the tab's [`QueryEngine`] state on top of `<name>`.
//!
//! Every applied step is recorded in one list, so the steps panel and undo can
//! never disagree (the Python app kept two parallel lists).

use std::path::{Path, PathBuf};

use duckdb::Connection;
use serde::Serialize;

use crate::edits::{CellEdit, EditQueue, overlay_sql, validate_value};
use crate::engine::{QueryEngine, describe};
use crate::error::{CoreError, Result};
use crate::export::{ExportOptions, export};
use crate::format::{FileFormat, register_source};
use crate::types::{CsvOptions, DetectOptions, Reading, SourceColumn};
use crate::colfn::{ColumnFn, JsonKey};
use crate::formula::{Formula, formula_sql};
use crate::ops::{BuiltOp, OpContext, Operation, StepLabel};
use crate::page::{Cell, ColumnMeta, Page};
use crate::search::{SearchHit, SearchOptions, find_next};
use crate::sql::{quote_ident, quote_literal, slugify_name};
use crate::stats::{ColumnStats, SampleMode, column_sample, column_stats};

pub type TabId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
enum StepKind {
    Sql,
    /// Cells edited (one cell, or several at once by a fill); keeps each
    /// cell's previous pending edit so undo restores it exactly.
    CellEdits { previous: Vec<(u64, String, Option<CellEdit>)> },
    /// A column of the CSV file is read another way; keeps the previous
    /// reading for undo. The query is unchanged, so cell edits stay allowed.
    SourceReading { previous: SourceColumn },
    /// The tab's formulas changed; keeps the previous list for undo. The
    /// query is unchanged, so cell edits stay allowed.
    Formulas { previous: Vec<Formula> },
}

#[derive(Debug, Clone)]
struct Step {
    label: StepLabel,
    kind: StepKind,
}

#[derive(Debug)]
struct Tab {
    id: TabId,
    name: String,
    path: PathBuf,
    format: FileFormat,
    engine: QueryEngine,
    edits: EditQueue,
    steps: Vec<Step>,
    /// How each column of a CSV file is read (empty for other formats).
    source: Vec<SourceColumn>,
    /// How the CSV file is parsed ("Open with options").
    csv: CsvOptions,
    /// Column formulas, in order (live: between the cell edits and the steps).
    formulas: Vec<Formula>,
    /// The worksheet shown, as named in the file (Excel only).
    sheet: Option<String>,
    /// A table operation's result: where it came from (until saved).
    origin: Option<Origin>,
    /// The in-memory table holding a result's rows (until saved).
    result_table: Option<String>,
}

impl Tab {
    fn source_view(&self) -> String {
        source_view(&self.name)
    }
}

fn source_view(name: &str) -> String {
    format!("__pv_src_{name}")
}

/// The file plus pending cell edits; formulas are added on top of it.
fn edits_view(name: &str) -> String {
    format!("__pv_edits_{name}")
}

/// Create the edits view (`src` + `edits`) and the table's view (edits +
/// `formulas`) of the tab called `name`.
fn create_views(conn: &Connection, name: &str, edits: &[CellEdit], formulas: &[Formula]) -> Result<()> {
    let src = source_view(name);
    let src_columns = describe(conn, &format!("SELECT * FROM {}", quote_ident(&src)))?;
    let overlay = overlay_sql(&src, &src_columns, edits)?;
    let base = edits_view(name);
    conn.execute_batch(&format!("CREATE OR REPLACE VIEW {} AS {overlay}", quote_ident(&base)))?;
    // Column totals (SUM([x])) are computed here, once per rebuild.
    let formulas = crate::formula::with_totals(conn, &base, name, formulas)?;
    let table = formula_sql(&base, name, &src_columns, &formulas);
    conn.execute_batch(&format!("CREATE OR REPLACE VIEW {} AS {table}", quote_ident(name)))?;
    Ok(())
}


fn excel_table(name: &str) -> String {
    format!("__pv_xlsx_{name}")
}

/// Result of [`Workspace::preview_op`].
#[derive(Debug, Clone, Serialize)]
pub struct OpPreview {
    pub label: StepLabel,
    pub sql: String,
    pub rows_before: u64,
    pub rows_after: u64,
    pub columns: Vec<crate::page::ColumnMeta>,
    pub rows: Vec<Vec<crate::page::Cell>>,
}

/// Rows a table operation's result may have in memory. A bigger result is
/// written to a file instead ([`Materialized::TooBig`]).
pub const RESULT_ROW_LIMIT: u64 = 5_000_000;

/// Names of the in-memory tables holding results (unique per process).
static NEXT_RESULT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// How a result tab was made from its source.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Derivation {
    /// A table operation (Group By, Pivot, Join…).
    Op { op: Operation },
    /// A SQL query run "into a new tab".
    Sql { sql: String },
}

/// Where a result tab came from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Origin {
    pub source_id: TabId,
    /// The source tab's table name.
    pub source: String,
    /// What was done, e.g. "Group By region".
    pub label: StepLabel,
    pub derivation: Derivation,
}

/// A result to compute: [`Workspace::plan_result`], then [`materialize`] on
/// any connection (a job), then [`Workspace::add_result`] or
/// [`Workspace::replace_result`].
#[derive(Debug, Clone)]
pub struct ResultPlan {
    /// The result's SQL over the source's current data.
    pub sql: String,
    /// The in-memory table to fill.
    pub table: String,
    /// The new tab's table name.
    pub name: String,
    pub origin: Origin,
}

/// What [`materialize`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "rows", rename_all = "snake_case")]
pub enum Materialized {
    /// The result table holds this many rows.
    Rows(u64),
    /// More than [`RESULT_ROW_LIMIT`] rows: nothing was kept.
    TooBig,
}

/// Compute `plan` into its in-memory table. Reads at most one row past the
/// limit, so a huge join stops early instead of filling memory.
pub fn materialize(conn: &Connection, plan: &ResultPlan) -> Result<Materialized> {
    let table = quote_ident(&plan.table);
    conn.execute_batch(&format!(
        "CREATE TABLE {table} AS SELECT * FROM ({}) LIMIT {}",
        plan.sql,
        RESULT_ROW_LIMIT + 1
    ))?;
    let rows: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
    if rows as u64 > RESULT_ROW_LIMIT {
        conn.execute_batch(&format!("DROP TABLE IF EXISTS {table}"))?;
        return Ok(Materialized::TooBig);
    }
    Ok(Materialized::Rows(rows.max(0) as u64))
}

/// Drop a result table that will not be used (a cancelled or failed job).
pub fn discard_result(conn: &Connection, plan: &ResultPlan) {
    let _ = conn.execute_batch(&format!("DROP TABLE IF EXISTS {}", quote_ident(&plan.table)));
}

/// Everything the UI shows about a tab apart from the data itself.
#[derive(Debug, Clone, Serialize)]
pub struct TabSummary {
    pub id: TabId,
    pub name: String,
    pub path: PathBuf,
    pub format: FileFormat,
    pub file_size: u64,
    pub total_rows: u64,
    pub total_pages: u64,
    pub page_size: u64,
    pub is_base_query: bool,
    pub can_undo: bool,
    pub current_query: String,
    pub steps: Vec<StepLabel>,
    pub pending_edits: usize,
    /// Column formulas of the table.
    pub formulas: Vec<Formula>,
    /// The worksheet shown (Excel), as it will be named after saving.
    pub sheet: Option<String>,
    /// The workbook's sheets were renamed, hidden, deleted… and not saved.
    pub sheets_changed: bool,
    /// The worksheet's name in the file as stored (for per-sheet settings).
    #[serde(skip)]
    pub stored_sheet: Option<String>,
    /// A table operation's result not saved to a file yet: where it came
    /// from. Its `path` is empty.
    pub origin: Option<Origin>,
}

/// The sheets of the workbook a tab shows, for its sheet menu.
#[derive(Debug, Clone, Serialize)]
pub struct SheetsView {
    /// The sheet this tab shows (its name after saving).
    pub current: String,
    pub sheets: Vec<crate::workbook::PlanSheet>,
    /// Sheets (by stored name) open in some tab of this window.
    pub open: Vec<String>,
    pub changed: bool,
}

/// The key of a workbook's pending sheet changes.
fn book_key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// What "Open with options" shows before opening.
#[derive(Debug, Clone, Serialize)]
pub struct CsvPreview {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<Cell>>,
    /// DuckDB's own guess (delimiter, header, rows to skip), when it has one.
    pub detected: Option<CsvOptions>,
}

/// Default cap on DuckDB's working memory. Larger sorts, joins and
/// aggregations spill to the temp directory instead of growing the process.
pub const DEFAULT_MEMORY_LIMIT_MB: u64 = 1024;

/// Most threads DuckDB may use. More threads speed up large scans only a
/// little but each one holds its own buffers.
pub const MAX_THREADS: usize = 8;

/// Memory each DuckDB thread gets. Every thread reading a Parquet file holds
/// a decoded column chunk of its row group; on wide files with big row
/// groups that is 100–150 MB, so 8 threads did not fit in a 1 GB limit
/// (a filter's first page failed with "could not allocate block").
const MB_PER_THREAD: u64 = 256;

/// Threads for a memory limit: one per 256 MB, at most one per core and
/// [`MAX_THREADS`].
pub fn threads_for(memory_limit_mb: u64) -> usize {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    ((memory_limit_mb / MB_PER_THREAD) as usize).clamp(1, cores.clamp(1, MAX_THREADS))
}

/// DuckDB settings that keep memory use bounded and predictable.
///
/// DuckDB's own defaults (80% of RAM, one thread per core) suit a database
/// server; a desktop viewer shares the machine with other programs. The
/// memory limit can be changed with the `PARVU_MEMORY_LIMIT_MB` environment
/// variable.
fn resource_settings() -> String {
    let limit = std::env::var("PARVU_MEMORY_LIMIT_MB")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v >= 128)
        .unwrap_or(DEFAULT_MEMORY_LIMIT_MB);
    let threads = threads_for(limit);
    // Return freed memory to the system quickly instead of keeping it
    // cached in the allocator after a large query.
    format!(
        "SET threads = {threads}; SET memory_limit = '{limit}MB'; \
         SET allocator_flush_threshold = '32MB'; SET allocator_background_threads = true; {SAFE_CASTS}"
    )
}

/// DuckDB's `type_pushdown` optimizer moves a `TRY_CAST` over a CSV text
/// column into the CSV reader, which then parses the column strictly: one
/// bad value fails the whole query instead of becoming NULL (DuckDB 2.0).
/// ParVu's conversions rely on `TRY_CAST`, so the rule stays off.
const SAFE_CASTS: &str = "SET disabled_optimizers = 'type_pushdown';";

/// Settings every connection to the workspace's database needs.
fn connection_settings(conn: &Connection) -> Result<()> {
    conn.execute_batch(SAFE_CASTS)?;
    Ok(())
}

pub struct Workspace {
    conn: Connection,
    tabs: Vec<Tab>,
    next_id: TabId,
    page_size: u64,
    /// How files opened from now on are typed.
    detect: DetectOptions,
    /// Sheet changes not saved yet, by workbook.
    workbooks: std::collections::HashMap<PathBuf, crate::workbook::WorkbookPlan>,
}

impl Workspace {
    /// New in-memory workspace. `temp_dir` is where DuckDB spills when a query
    /// needs more memory than is available.
    pub fn new(page_size: u64, temp_dir: Option<&Path>) -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        crate::interrupts::register(&conn);
        conn.execute_batch(&resource_settings())?;
        if let Some(dir) = temp_dir {
            std::fs::create_dir_all(dir)?;
            conn.execute_batch(&format!(
                "SET temp_directory = {}",
                crate::sql::quote_literal(&dir.to_string_lossy())
            ))?;
        }
        Ok(Self {
            conn,
            tabs: Vec::new(),
            next_id: 1,
            page_size: page_size.max(1),
            detect: DetectOptions::default(),
            workbooks: Default::default(),
        })
    }

    /// How files opened from now on are typed (detection on or off, the
    /// order of day and month in ambiguous dates).
    pub fn set_detect_options(&mut self, options: DetectOptions) {
        self.detect = options;
    }

    /// How each column of a tab's CSV file is read; empty for other formats.
    pub fn source_columns(&self, id: TabId) -> Result<&[SourceColumn]> {
        Ok(&self.tab(id)?.source)
    }

    /// After a query failed because DuckDB couldn't parse a CSV value deep
    /// in a file (past the rows its types were guessed from): switch that
    /// column of that file to text + `TRY_CAST`, so the value becomes NULL.
    /// Returns true when something changed and the query is worth retrying.
    pub fn heal(&mut self, message: &str) -> bool {
        let Some((file, column)) = crate::types::heal_target(message) else { return false };
        let same = |p: &Path| p.to_string_lossy() == file || std::fs::canonicalize(p).ok() == std::fs::canonicalize(&file).ok();
        let mut healed = false;
        for tab in self.tabs.iter_mut().filter(|t| t.format == FileFormat::Csv && same(&t.path)) {
            let Some(col) = tab.source.iter_mut().find(|c| c.name == column && c.native) else { continue };
            col.native = false;
            let select = match crate::types::csv_select(&tab.path.to_string_lossy(), &tab.source, &tab.csv) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("could not re-read {column} as text: {e}");
                    continue;
                }
            };
            match self.conn.execute_batch(&format!("CREATE OR REPLACE VIEW {} AS {select}", quote_ident(&source_view(&tab.name)))) {
                Ok(()) => {
                    tracing::info!("{}: column {column} now read as text (a value didn't parse)", tab.name);
                    healed = true;
                }
                Err(e) => tracing::warn!("could not re-read {column} as text: {e}"),
            }
        }
        healed
    }

    /// The first `n` rows of the CSV file at `path` read with `csv`, for the
    /// "Open with options" preview, and what DuckDB detects on its own.
    pub fn preview_csv(&self, path: &Path, csv: &CsvOptions, n: u64) -> Result<CsvPreview> {
        let p = quote_literal(&path.to_string_lossy());
        let query = format!("SELECT * FROM read_csv({p}{}) LIMIT {n}", csv.params()?);
        let columns = crate::engine::describe(&self.conn, &query)?;
        let rows = crate::page::format_batches(&crate::engine::query_batches(&self.conn, &query)?)?;
        let detected = self
            .conn
            .query_row(&format!("SELECT Delimiter, HasHeader, SkipRows FROM sniff_csv({p})"), [], |r| {
                Ok(CsvOptions {
                    delimiter: r.get::<_, Option<String>>(0)?,
                    header: r.get::<_, Option<bool>>(1)?,
                    skip: r.get::<_, Option<i64>>(2)?.map(|v| v.max(0) as u32),
                    decimal_separator: None,
                    encoding: None,
                })
            })
            .ok();
        Ok(CsvPreview { columns, rows, detected })
    }

    /// Read `column` of a tab's CSV file as `reading` (the column type
    /// menu). Undoable. Fails, changing nothing, when the current query or
    /// pending cell edits don't work with the new type.
    pub fn set_source_reading(&mut self, id: TabId, column: &str, reading: Reading) -> Result<StepLabel> {
        let tab = self.tab(id)?;
        let previous = tab
            .source
            .iter()
            .find(|c| c.name == column)
            .cloned()
            .ok_or_else(|| CoreError::invalid(format!("No column \"{column}\" in the file")))?;
        let next = SourceColumn { name: column.to_string(), reading, native: false, ambiguous: false, stored: previous.stored.clone() };
        let label = StepLabel::new("step.source_type", &[("column", column), ("type", next.reading.dtype())]);
        self.replace_source_columns(id, std::slice::from_ref(&next))?;
        let tab = self.tab_mut(id)?;
        tab.steps.push(Step { label: label.clone(), kind: StepKind::SourceReading { previous } });
        Ok(label)
    }

    /// Read some columns of a tab's CSV file another way without a step
    /// (types the user chose for this file before). Unknown columns are
    /// skipped; a set that doesn't work changes nothing.
    pub fn apply_source_readings(&mut self, id: TabId, readings: &[(String, Reading)]) -> Result<()> {
        let tab = self.tab(id)?;
        let next: Vec<SourceColumn> = readings
            .iter()
            .filter_map(|(name, reading)| {
                let old = tab.source.iter().find(|c| &c.name == name)?;
                Some(SourceColumn { name: name.clone(), reading: reading.clone(), native: false, ambiguous: false, stored: old.stored.clone() })
            })
            .collect();
        if next.is_empty() {
            return Ok(());
        }
        self.replace_source_columns(id, &next)
    }

    /// Swap in `columns` (by name), rebuild the source and edit views, and
    /// check the current query still runs; restore everything otherwise.
    fn replace_source_columns(&mut self, id: TabId, columns: &[SourceColumn]) -> Result<()> {
        let conn = &self.conn;
        let tab = self.tabs.iter_mut().find(|t| t.id == id).ok_or(CoreError::NoSuchTab(id))?;
        let before = tab.source.clone();
        for c in columns {
            if let Some(slot) = tab.source.iter_mut().find(|s| s.name == c.name) {
                *slot = c.clone();
            }
        }
        let rebuild = |tab: &mut Tab| -> Result<()> {
            let select = match crate::format::base_select(tab.format, &tab.path, &excel_table(&tab.name)) {
                Some(base) => crate::types::typed_select(&base, &tab.source),
                None => crate::types::csv_select(&tab.path.to_string_lossy(), &tab.source, &tab.csv)?,
            };
            conn.execute_batch(&format!("CREATE OR REPLACE VIEW {} AS {select}", quote_ident(&tab.source_view())))?;
            rebuild_overlay(conn, tab)?;
            crate::engine::validate(conn, tab.engine.current_query())
        };
        if let Err(e) = rebuild(tab) {
            tab.source = before;
            if let Err(e2) = rebuild(tab) {
                tracing::warn!("could not restore the file's column types: {e2}");
            }
            return Err(e);
        }
        Ok(())
    }

    /// The columns formulas can use: the file's (with cell edits) and the
    /// formulas', in table order. Steps don't change them.
    /// Rows of the table under the steps (the file with its cell edits).
    pub fn table_rows(&self, id: TabId) -> Result<u64> {
        let t = self.tab(id)?;
        if t.engine.is_base_query() {
            return Ok(t.engine.total_rows());
        }
        let n: i64 = self.conn.query_row(&format!("SELECT COUNT(*) FROM {}", quote_ident(&edits_view(&t.name))), [], |r| r.get(0))?;
        Ok(n as u64)
    }

    pub fn formula_columns(&self, id: TabId) -> Result<Vec<ColumnMeta>> {
        let t = self.tab(id)?;
        describe(&self.conn, &format!("SELECT * FROM {}", quote_ident(&t.name)))
    }

    /// Add or change (by name) a column formula. Undoable; the query is
    /// unchanged, so cell edits stay allowed. Fails, changing nothing, when
    /// the formula doesn't run or a later step can't use the new column.
    pub fn set_formula(&mut self, id: TabId, formula: Formula) -> Result<StepLabel> {
        let name = formula.name.trim().to_string();
        if name.is_empty() {
            return Err(CoreError::invalid("Enter a name for the formula column"));
        }
        let t = self.tab(id)?;
        let previous = t.formulas.clone();
        let existing = previous.iter().position(|f| f.name == name);
        if existing.is_none() {
            let src_columns = describe(&self.conn, &format!("SELECT * FROM {}", quote_ident(&edits_view(&t.name))))?;
            if src_columns.iter().any(|c| c.name == name) {
                return Err(CoreError::invalid(format!("\"{name}\" is already a column of the file")));
            }
        }
        let mut next = previous.clone();
        let formula = Formula { name: name.clone(), ..formula };
        match existing {
            Some(i) => next[i] = formula,
            None => next.push(formula),
        }
        self.replace_formulas(id, next)?;
        let key = if existing.is_some() { "step.formula_edit" } else { "step.formula_add" };
        let label = StepLabel::new(key, &[("column", &name)]);
        self.tab_mut(id)?.steps.push(Step { label: label.clone(), kind: StepKind::Formulas { previous } });
        Ok(label)
    }

    /// The table's first `n` rows as they would be with `formula` added (or
    /// changed, by name), without changing anything: for approval cards.
    pub fn preview_formula(&self, id: TabId, formula: &Formula, n: u64) -> Result<(Vec<ColumnMeta>, Vec<Vec<Cell>>)> {
        let t = self.tab(id)?;
        let mut list = t.formulas.clone();
        match list.iter().position(|f| f.name == formula.name) {
            Some(i) => list[i] = formula.clone(),
            None => list.push(formula.clone()),
        }
        let base = edits_view(&t.name);
        let src_columns = describe(&self.conn, &format!("SELECT * FROM {}", quote_ident(&source_view(&t.name))))?;
        let resolved = crate::formula::with_totals(&self.conn, &base, &t.name, &list)?;
        let sql = format!("{} LIMIT {n}", formula_sql(&base, &t.name, &src_columns, &resolved));
        let columns = describe(&self.conn, &sql)?;
        let rows = crate::page::format_batches(&crate::engine::query_batches(&self.conn, &sql)?)?;
        Ok((columns, rows))
    }

    /// Put back formulas remembered for the file when it opens again: no
    /// undo step (they are part of the table, like "Read as" choices).
    /// Fails, changing nothing, when they don't run.
    pub fn restore_formulas(&mut self, id: TabId, formulas: Vec<Formula>) -> Result<()> {
        self.replace_formulas(id, formulas)
    }

    /// Remove a formula column. Undoable; refused while other formulas or
    /// steps use it.
    pub fn remove_formula(&mut self, id: TabId, name: &str) -> Result<StepLabel> {
        let previous = self.tab(id)?.formulas.clone();
        if !previous.iter().any(|f| f.name == name) {
            return Err(CoreError::invalid(format!("\"{name}\" is not a formula column")));
        }
        let next: Vec<Formula> = previous.iter().filter(|f| f.name != name).cloned().collect();
        self.replace_formulas(id, next).map_err(|e| {
            CoreError::invalid(format!("\"{name}\" is used by another formula or a later step ({e})"))
        })?;
        let label = StepLabel::new("step.formula_remove", &[("column", name)]);
        self.tab_mut(id)?.steps.push(Step { label: label.clone(), kind: StepKind::Formulas { previous } });
        Ok(label)
    }

    /// Swap in `formulas`, rebuild the views and check the current query
    /// still runs; restore everything otherwise.
    fn replace_formulas(&mut self, id: TabId, formulas: Vec<Formula>) -> Result<()> {
        let conn = &self.conn;
        let tab = self.tabs.iter_mut().find(|t| t.id == id).ok_or(CoreError::NoSuchTab(id))?;
        let before = std::mem::replace(&mut tab.formulas, formulas);
        let rebuild = |tab: &mut Tab| -> Result<()> {
            rebuild_overlay(conn, tab)?;
            crate::engine::validate(conn, tab.engine.current_query())
        };
        if let Err(e) = rebuild(tab) {
            tab.formulas = before;
            if let Err(e2) = rebuild(tab) {
                tracing::warn!("could not restore the formulas: {e2}");
            }
            return Err(e);
        }
        Ok(())
    }

    /// Run `f`, healing and retrying when it fails on an unparsable CSV
    /// value (see [`Workspace::heal`]).
    pub fn healing<T>(&mut self, mut f: impl FnMut(&mut Self) -> Result<T>) -> Result<T> {
        let mut tries = 0;
        loop {
            match f(self) {
                Err(e) if tries < 64 && self.heal(&e.to_string()) => tries += 1,
                other => return other,
            }
        }
    }

    /// Change DuckDB's working-memory limit (MB, at least 128).
    pub fn set_memory_limit_mb(&self, mb: u64) -> Result<()> {
        let mb = mb.max(128);
        self.conn.execute_batch(&format!("SET memory_limit = '{mb}MB'; SET threads = {}", threads_for(mb)))?;
        Ok(())
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// A second connection to the same database, for work on another thread
    /// (exports, long statistics) while the UI keeps paging.
    pub fn clone_connection(&self) -> Result<Connection> {
        let conn = self.conn.try_clone()?;
        crate::interrupts::register(&conn);
        connection_settings(&conn)?;
        Ok(conn)
    }

    pub fn page_size(&self) -> u64 {
        self.page_size
    }

    pub fn set_page_size(&mut self, page_size: u64) {
        self.page_size = page_size.max(1);
        for t in &mut self.tabs {
            t.engine.set_page_size(self.page_size);
        }
    }

    fn tab(&self, id: TabId) -> Result<&Tab> {
        self.tabs.iter().find(|t| t.id == id).ok_or(CoreError::NoSuchTab(id))
    }

    fn tab_mut(&mut self, id: TabId) -> Result<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id).ok_or(CoreError::NoSuchTab(id))
    }

    pub fn tab_ids(&self) -> Vec<TabId> {
        self.tabs.iter().map(|t| t.id).collect()
    }

    pub fn tab_name(&self, id: TabId) -> Result<&str> {
        Ok(&self.tab(id)?.name)
    }

    /// Open `path` as a new tab. The table name is the slugified file stem,
    /// made unique with `_2`, `_3`, ... when needed.
    pub fn open(&mut self, path: &Path) -> Result<TabId> {
        self.open_with(path, &CsvOptions::default())
    }

    /// Open `path` like [`Workspace::open`], reading a CSV file with `csv`
    /// (delimiter, header, rows to skip…; ignored for other formats).
    pub fn open_with(&mut self, path: &Path, csv: &CsvOptions) -> Result<TabId> {
        self.open_sheet(path, csv, None)
    }

    /// Open `path` like [`Workspace::open_with`]; for an Excel workbook,
    /// show `sheet` (by its stored name), or the first visible one. A sheet
    /// asked for by name gets a table name with the sheet's: `sales_q1`.
    pub fn open_sheet(&mut self, path: &Path, csv: &CsvOptions, sheet: Option<&str>) -> Result<TabId> {
        let path = path.to_path_buf();
        if !path.is_file() {
            return Err(CoreError::FileNotFound(path.display().to_string()));
        }
        let format = FileFormat::require(&path)?;
        let sheet = match (format, sheet) {
            (FileFormat::Excel, Some(s)) => Some(s.to_string()),
            (FileFormat::Excel, None) => Some(crate::workbook::first_sheet(&path)?),
            _ => None,
        };
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("data");
        let explicit = format == FileFormat::Excel && sheet.is_some() && sheet != Some(crate::workbook::first_sheet(&path)?);
        let base = if explicit {
            slugify_name(&format!("{stem}_{}", sheet.as_deref().unwrap_or_default()))
        } else {
            slugify_name(stem)
        };
        let mut name = base.clone();
        let mut n = 2;
        while self.tabs.iter().any(|t| t.name == name) {
            name = format!("{base}_{n}");
            n += 1;
        }

        let src = source_view(&name);
        let source = match register_source(&self.conn, &src, &excel_table(&name), &path, self.detect, csv, sheet.as_deref()) {
            Ok(s) => s,
            Err(e) => {
                self.drop_relations(&name);
                return Err(e);
            }
        };
        let result = (|| {
            create_views(&self.conn, &name, &[], &[])?;
            QueryEngine::new(&self.conn, &name, self.page_size)
        })();
        let engine = match result {
            Ok(e) => e,
            Err(e) => {
                self.drop_relations(&name);
                return Err(e);
            }
        };
        let id = self.next_id;
        self.next_id += 1;
        tracing::info!("opened {} as {name} ({} rows)", path.display(), engine.total_rows());
        self.tabs.push(Tab {
            id,
            name,
            path,
            format,
            engine,
            edits: EditQueue::new(),
            steps: Vec::new(),
            source,
            csv: csv.clone(),
            formulas: Vec::new(),
            sheet,
            origin: None,
            result_table: None,
        });
        Ok(id)
    }

    fn drop_relations(&self, name: &str) {
        let _ = self.conn.execute_batch(&format!(
            "DROP VIEW IF EXISTS {}; DROP VIEW IF EXISTS {}; DROP VIEW IF EXISTS {}; DROP TABLE IF EXISTS {};",
            quote_ident(name),
            quote_ident(&edits_view(name)),
            quote_ident(&source_view(name)),
            quote_ident(&excel_table(name))
        ));
    }

    pub fn close(&mut self, id: TabId) -> Result<()> {
        let idx = self.tabs.iter().position(|t| t.id == id).ok_or(CoreError::NoSuchTab(id))?;
        let tab = self.tabs.remove(idx);
        self.drop_relations(&tab.name);
        if let Some(table) = &tab.result_table {
            let _ = self.conn.execute_batch(&format!("DROP TABLE IF EXISTS {}", quote_ident(table)));
        }
        // Sheet changes of a workbook no tab shows any more are dropped.
        let key = book_key(&tab.path);
        if !self.tabs.iter().any(|t| book_key(&t.path) == key) {
            self.workbooks.remove(&key);
        }
        Ok(())
    }

    pub fn summary(&self, id: TabId) -> Result<TabSummary> {
        let t = self.tab(id)?;
        Ok(TabSummary {
            id: t.id,
            name: t.name.clone(),
            path: t.path.clone(),
            format: t.format,
            file_size: std::fs::metadata(&t.path).map(|m| m.len()).unwrap_or(0),
            total_rows: t.engine.total_rows(),
            total_pages: t.engine.total_pages(),
            page_size: t.engine.page_size(),
            is_base_query: t.engine.is_base_query(),
            can_undo: !t.steps.is_empty(),
            current_query: t.engine.current_query().to_string(),
            steps: t.steps.iter().map(|s| s.label.clone()).collect(),
            pending_edits: t.edits.len(),
            formulas: t.formulas.clone(),
            sheet: t.sheet.as_ref().map(|s| {
                let plan = self.workbooks.get(&book_key(&t.path));
                plan.and_then(|p| p.saved_name(s)).unwrap_or(s).to_string()
            }),
            sheets_changed: t.sheet.is_some() && self.workbooks.contains_key(&book_key(&t.path)),
            stored_sheet: t.sheet.clone(),
            origin: t.origin.clone(),
        })
    }

    pub fn summaries(&self) -> Vec<TabSummary> {
        self.tabs.iter().filter_map(|t| self.summary(t.id).ok()).collect()
    }

    pub fn columns(&self, id: TabId) -> Result<Vec<ColumnMeta>> {
        self.tab(id)?.engine.columns(&self.conn)
    }

    /// Current query of the tab named `name` (used by joins and appends).
    pub fn current_query_of(&self, name: &str) -> Option<String> {
        self.tabs.iter().find(|t| t.name == name).map(|t| t.engine.current_query().to_string())
    }

    pub fn current_query(&self, id: TabId) -> Result<String> {
        Ok(self.tab(id)?.engine.current_query().to_string())
    }

    /// Every table with its current columns, for DSL autocomplete and resolution.
    pub fn catalog(&self) -> Vec<(String, Vec<ColumnMeta>)> {
        self.tabs
            .iter()
            .filter_map(|t| t.engine.columns(&self.conn).ok().map(|c| (t.name.clone(), c)))
            .collect()
    }

    /// One page of the tab's current result, with pending edits marked.
    pub fn page(&self, id: TabId, page: u64) -> Result<Page> {
        let t = self.tab(id)?;
        let (page, offset, columns, rows) = t.engine.fetch(&self.conn, page)?;
        let editable = t.engine.is_base_query();
        let mut edited = Vec::new();
        if editable {
            for e in t.edits.all() {
                if e.row >= offset
                    && e.row < offset + rows.len() as u64
                    && let Some(ci) = columns.iter().position(|c| c.name == e.column)
                {
                    edited.push([(e.row - offset) as u32, ci as u32]);
                }
            }
        }
        Ok(Page {
            page,
            total_pages: t.engine.total_pages(),
            total_rows: t.engine.total_rows(),
            page_size: t.engine.page_size(),
            offset,
            columns,
            rows,
            edited,
            editable,
        })
    }

    /// Run user SQL as the tab's new query. Recorded as a step so undo works.
    pub fn run_sql(&mut self, id: TabId, sql: &str, label: Option<StepLabel>) -> Result<StepLabel> {
        let label = label.unwrap_or_else(|| {
            let short: String = sql.split_whitespace().collect::<Vec<_>>().join(" ");
            let short = if short.chars().count() > 60 {
                format!("{}…", short.chars().take(60).collect::<String>())
            } else {
                short
            };
            StepLabel::new("step.query", &[("sql", &short)])
        });
        let conn = &self.conn;
        let tab = self.tabs.iter_mut().find(|t| t.id == id).ok_or(CoreError::NoSuchTab(id))?;
        tab.engine.set_query(conn, sql)?;
        tab.steps.push(Step { label: label.clone(), kind: StepKind::Sql });
        Ok(label)
    }

    /// Run SQL in which the tab's own name refers to its current result
    /// rather than the original file (used for expression mode).
    pub fn run_relative_sql(&mut self, id: TabId, sql: &str, label: StepLabel) -> Result<StepLabel> {
        let t = self.tab(id)?;
        let wrapped = format!("WITH {} AS ({}) {sql}", quote_ident(&t.name), t.engine.current_query());
        self.run_sql(id, &wrapped, Some(label))
    }

    /// The SQL and step label an operation would produce on the tab, without
    /// applying it.
    pub fn build_op(&self, id: TabId, op: &Operation) -> Result<BuiltOp> {
        let t = self.tab(id)?;
        let columns = t.engine.columns(&self.conn)?;
        let other = |name: &str| self.current_query_of(name);
        // A sort right after another sort replaces it instead of wrapping
        // it: stacked ORDER BYs make DuckDB sort the whole result twice.
        // The earlier sort stays in the history, so undo still returns to it.
        let last_was_sort = t.steps.last().is_some_and(|s| {
            s.kind == StepKind::Sql && (s.label.key == "step.sort" || s.label.key == "step.sort_by")
        });
        let is_sort = matches!(op, Operation::Sort { .. } | Operation::SortBy { .. });
        let source = match (is_sort, last_was_sort, t.engine.previous_query()) {
            (true, true, Some(before_sort)) => before_sort,
            _ => t.engine.current_query(),
        };
        let conn = &self.conn;
        let query = |sql: &str| crate::page::format_batches(&crate::engine::query_batches(conn, sql)?);
        op.build(&OpContext {
            table: &t.name,
            current_query: source,
            columns: &columns,
            other_query: &other,
            query: &query,
        })
    }

    /// The context operations are built in, for the tab's current result.
    fn with_ctx<T>(&self, id: TabId, f: impl FnOnce(&OpContext) -> Result<T>) -> Result<T> {
        let t = self.tab(id)?;
        let columns = t.engine.columns(&self.conn)?;
        let other = |name: &str| self.current_query_of(name);
        let conn = &self.conn;
        let query = |sql: &str| crate::page::format_batches(&crate::engine::query_batches(conn, sql)?);
        f(&OpContext { table: &t.name, current_query: t.engine.current_query(), columns: &columns, other_query: &other, query: &query })
    }

    /// The first `n` values of `column` before and after reading them as
    /// `to`, and whether each failed (a non-empty value that doesn't convert).
    pub fn preview_conversion(&self, id: TabId, column: &str, to: &Reading, n: u64) -> Result<Vec<(Cell, Cell, bool)>> {
        let sql = self.with_ctx(id, |ctx| {
            let (conv, failed) = crate::ops::conversion(ctx, column, to)?;
            Ok(format!(
                "SELECT CAST({c} AS VARCHAR), CAST({conv} AS VARCHAR), {failed} FROM ({}) LIMIT {n}",
                ctx.current_query,
                c = quote_ident(column)
            ))
        })?;
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, bool>(2)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// `values` of `column` (as shown on the page) before and after `func`:
    /// the preview of the column function dialogs. Runs over the values
    /// alone, so it costs nothing whatever the size of the table.
    pub fn preview_column_fn(&self, id: TabId, column: &str, func: &ColumnFn, values: &[Cell]) -> Result<Vec<(Cell, Cell)>> {
        let dtype = self.with_ctx(id, |ctx| Ok(ctx.column(column)?.dtype.clone()))?;
        if values.is_empty() {
            return Ok(Vec::new());
        }
        let expr = func.expr(column, &dtype)?;
        let ty = if crate::sql::is_safe_type_name(&dtype) { dtype.as_str() } else { "VARCHAR" };
        let rows: Vec<String> = values
            .iter()
            .map(|v| format!("({})", v.as_deref().map(quote_literal).unwrap_or_else(|| "NULL".into())))
            .collect();
        let sql = format!(
            "SELECT v, CAST({expr} AS VARCHAR) FROM (SELECT v, TRY_CAST(v AS {ty}) AS {c} FROM (VALUES {}) t(v))",
            rows.join(", "),
            c = quote_ident(column)
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let out = stmt
            .query_map([], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(out)
    }

    /// The keys of the JSON objects in `column` and their types, from the
    /// first `limit` rows (or all), for "Expand into columns".
    pub fn json_keys(&self, id: TabId, column: &str, limit: Option<u64>) -> Result<Vec<JsonKey>> {
        json_keys_from(&self.conn, &self.json_keys_query(id, column, limit)?)
    }

    /// SQL for [`Workspace::json_keys`]; run it with [`json_keys_from`].
    pub fn json_keys_query(&self, id: TabId, column: &str, limit: Option<u64>) -> Result<String> {
        self.with_ctx(id, |ctx| {
            ctx.require(column)?;
            let c = quote_ident(column);
            let source = match limit {
                Some(n) => format!("SELECT {c} FROM ({}) LIMIT {n}", ctx.current_query),
                None => format!("SELECT {c} FROM ({})", ctx.current_query),
            };
            Ok(format!("SELECT CAST(json_group_structure(TRY_CAST({c} AS JSON)) AS VARCHAR) FROM ({source})"))
        })
    }

    /// SQL counting the values of `column` that don't convert as `to`:
    /// one row `(errors, checked)`, over the first `limit` rows or all.
    /// Run it with [`conversion_errors`] (here, or on a cloned connection
    /// for a full count that shouldn't hold the workspace).
    pub fn conversion_error_query(&self, id: TabId, column: &str, to: &Reading, limit: Option<u64>) -> Result<String> {
        self.with_ctx(id, |ctx| {
            let (_, failed) = crate::ops::conversion(ctx, column, to)?;
            let source = match limit {
                Some(n) => format!("SELECT * FROM ({}) LIMIT {n}", ctx.current_query),
                None => ctx.current_query.to_string(),
            };
            Ok(format!("SELECT count(*) FILTER (WHERE {failed}), count(*) FROM ({source})"))
        })
    }

    /// How ParVu would read `column` of the current result, from its first
    /// rows as text (the conversion dialog's suggestion).
    pub fn suggest_reading(&self, id: TabId, column: &str) -> Result<Reading> {
        let query = self.with_ctx(id, |ctx| {
            ctx.require(column)?;
            Ok(format!("SELECT {} FROM ({})", quote_ident(column), ctx.current_query))
        })?;
        let found = crate::types::detect_query(&self.conn, &query, self.detect)?;
        let strict = found.into_iter().next().map(|c| c.reading).unwrap_or(Reading::Text);
        if strict != Reading::Text {
            return Ok(strict);
        }
        // No reading fits every value (a stray "n/a"): take the one that
        // converts the most of the first rows, if that's nearly all of them.
        let sample = format!("SELECT * FROM ({query}) LIMIT {}", crate::types::SAMPLE_ROWS);
        Ok(crate::types::best_reading(&self.conn, &sample, column, 0.9)?.unwrap_or(Reading::Text))
    }

    /// A table name not used by any tab: `base`, `base_2`…
    fn unique_name(&self, base: &str) -> String {
        let mut name = base.to_string();
        let mut n = 2;
        while self.tabs.iter().any(|t| t.name == name) {
            name = format!("{base}_{n}");
            n += 1;
        }
        name
    }

    /// Plan a result tab from tab `id`: a table operation or a SQL query.
    /// Checks that the SQL binds; nothing changes yet.
    pub fn plan_result(&self, id: TabId, derivation: Derivation) -> Result<ResultPlan> {
        let t = self.tab(id)?;
        let (sql, label, suffix) = match &derivation {
            Derivation::Op { op } => {
                if !op.is_table_op() {
                    return Err(CoreError::invalid("This operation changes the table itself; it doesn't make a new table"));
                }
                let built = self.build_op(id, op)?;
                (built.sql, built.label, op.result_suffix())
            }
            Derivation::Sql { sql } => {
                let short: String = sql.split_whitespace().collect::<Vec<_>>().join(" ");
                let short = if short.chars().count() > 60 { format!("{}…", short.chars().take(60).collect::<String>()) } else { short };
                (sql.trim().trim_end_matches(';').to_string(), StepLabel::new("step.query", &[("sql", &short)]), "query".into())
            }
        };
        crate::engine::validate(&self.conn, &sql)?;
        Ok(ResultPlan {
            sql,
            table: format!("__pv_res_{}", NEXT_RESULT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)),
            name: self.unique_name(&slugify_name(&format!("{}_{suffix}", t.name))),
            origin: Origin { source_id: t.id, source: t.name.clone(), label, derivation },
        })
    }

    /// Open a computed result (see [`materialize`]) as a new tab.
    pub fn add_result(&mut self, plan: ResultPlan) -> Result<TabId> {
        let name = self.unique_name(&plan.name);
        let src = source_view(&name);
        let result = (|| {
            self.conn.execute_batch(&format!(
                "CREATE OR REPLACE VIEW {} AS SELECT * FROM {}",
                quote_ident(&src),
                quote_ident(&plan.table)
            ))?;
            create_views(&self.conn, &name, &[], &[])?;
            QueryEngine::new(&self.conn, &name, self.page_size)
        })();
        let engine = match result {
            Ok(e) => e,
            Err(e) => {
                self.drop_relations(&name);
                discard_result(&self.conn, &plan);
                return Err(e);
            }
        };
        let id = self.next_id;
        self.next_id += 1;
        tracing::info!("result {name} from {} ({} rows)", plan.origin.source, engine.total_rows());
        self.tabs.push(Tab {
            id,
            name,
            path: PathBuf::new(),
            format: FileFormat::Parquet,
            engine,
            edits: EditQueue::new(),
            steps: Vec::new(),
            source: Vec::new(),
            csv: CsvOptions::default(),
            formulas: Vec::new(),
            sheet: None,
            origin: Some(plan.origin),
            result_table: Some(plan.table),
        });
        Ok(id)
    }

    /// The plan that computes result tab `id` again from its source's
    /// current data (Refresh).
    pub fn refresh_plan(&self, id: TabId) -> Result<ResultPlan> {
        let t = self.tab(id)?;
        let origin = t.origin.as_ref().ok_or_else(|| CoreError::invalid("This table was not made by a table operation"))?;
        if self.tab(origin.source_id).is_err() {
            return Err(CoreError::invalid(format!("The table it came from ({}) is closed", origin.source)));
        }
        let mut plan = self.plan_result(origin.source_id, origin.derivation.clone())?;
        plan.name = t.name.clone();
        Ok(plan)
    }

    /// Show a newly computed result in tab `id` (Refresh). Cell edits and
    /// steps are dropped; formulas are kept when they still apply.
    pub fn replace_result(&mut self, id: TabId, plan: ResultPlan) -> Result<()> {
        let (name, old, formulas) = {
            let t = self.tab(id)?;
            (t.name.clone(), t.result_table.clone(), t.formulas.clone())
        };
        let src = source_view(&name);
        let swap = (|| {
            self.conn.execute_batch(&format!(
                "CREATE OR REPLACE VIEW {} AS SELECT * FROM {}",
                quote_ident(&src),
                quote_ident(&plan.table)
            ))?;
            let formulas = match create_views(&self.conn, &name, &[], &formulas) {
                Ok(()) => formulas,
                Err(_) => {
                    create_views(&self.conn, &name, &[], &[])?;
                    Vec::new()
                }
            };
            Ok::<_, CoreError>((QueryEngine::new(&self.conn, &name, self.page_size)?, formulas))
        })();
        let (engine, formulas) = match swap {
            Ok(v) => v,
            Err(e) => {
                // Back to the previous rows.
                if let Some(old) = &old {
                    let _ = self.conn.execute_batch(&format!(
                        "CREATE OR REPLACE VIEW {} AS SELECT * FROM {}",
                        quote_ident(&src),
                        quote_ident(old)
                    ));
                    let t = self.tab(id)?;
                    let _ = create_views(&self.conn, &name, t.edits.all(), &t.formulas);
                }
                discard_result(&self.conn, &plan);
                return Err(e);
            }
        };
        if let Some(old) = old {
            let _ = self.conn.execute_batch(&format!("DROP TABLE IF EXISTS {}", quote_ident(&old)));
        }
        let tab = self.tab_mut(id)?;
        tab.engine = engine;
        tab.edits.clear();
        tab.steps.clear();
        tab.formulas = formulas;
        tab.origin = Some(plan.origin);
        tab.result_table = Some(plan.table);
        Ok(())
    }

    /// Apply a table operation as a new step.
    pub fn apply(&mut self, id: TabId, op: &Operation) -> Result<StepLabel> {
        let built = self.build_op(id, op)?;
        self.run_sql(id, &built.sql, Some(built.label))
    }

    /// What applying `op` would do: its label and SQL, the row count before
    /// and after, and the first `sample` rows of the result. Nothing changes.
    pub fn preview_op(&self, id: TabId, op: &Operation, sample: usize) -> Result<OpPreview> {
        let built = self.build_op(id, op)?;
        let t = self.tab(id)?;
        let conn = &self.conn;
        let limited = format!("SELECT * FROM ({}) LIMIT {sample}", built.sql);
        Ok(OpPreview {
            rows_before: t.engine.total_rows(),
            rows_after: crate::engine::count_rows(conn, &built.sql)?,
            columns: crate::engine::describe(conn, &built.sql)?,
            rows: crate::page::format_batches(&crate::engine::query_batches(conn, &limited)?)?,
            label: built.label,
            sql: built.sql,
        })
    }

    /// A read-only snapshot of every tab's current result on a separate
    /// connection, for the AI assistant and the MCP server. Hold the
    /// workspace only while creating it.
    pub fn read_session(&self) -> Result<crate::readonly::ReadSession> {
        let tables = self
            .tabs
            .iter()
            .map(|t| crate::readonly::SessionTable {
                name: t.name.clone(),
                query: t.engine.current_query().to_string(),
                steps: t.steps.iter().map(|s| s.label.clone()).collect(),
            })
            .collect();
        crate::readonly::ReadSession::new(self.clone_connection()?, tables)
    }

    /// Undo the most recent step (a transform or a cell edit).
    pub fn undo(&mut self, id: TabId) -> Result<StepLabel> {
        let conn = &self.conn;
        let tab = self.tabs.iter_mut().find(|t| t.id == id).ok_or(CoreError::NoSuchTab(id))?;
        let step = tab.steps.last().cloned().ok_or(CoreError::NothingToUndo)?;
        match &step.kind {
            StepKind::Sql => tab.engine.undo(conn)?,
            StepKind::CellEdits { previous } => {
                let before = tab.edits.clone();
                for (row, column, prev) in previous {
                    tab.edits.remove(*row, column);
                    if let Some(p) = prev {
                        tab.edits.add(p.clone());
                    }
                }
                if let Err(e) = rebuild_overlay(conn, tab) {
                    tab.edits = before;
                    return Err(e);
                }
            }
            StepKind::SourceReading { previous } => {
                self.replace_source_columns(id, std::slice::from_ref(previous))?;
            }
            StepKind::Formulas { previous } => {
                self.replace_formulas(id, previous.clone())?;
            }
        }
        let tab = self.tab_mut(id)?;
        tab.steps.pop();
        Ok(step.label)
    }

    /// Drop all transforms and return to the file view. Pending cell edits stay.
    pub fn reset(&mut self, id: TabId) -> Result<()> {
        let conn = &self.conn;
        let tab = self.tabs.iter_mut().find(|t| t.id == id).ok_or(CoreError::NoSuchTab(id))?;
        tab.engine.reset(conn)?;
        tab.steps.retain(|s| s.kind != StepKind::Sql);
        Ok(())
    }

    /// Edit one cell. Only allowed while the tab shows the unmodified file.
    /// An empty string stores NULL in non-text columns.
    pub fn edit_cell(&mut self, id: TabId, row: u64, column: &str, value: Cell) -> Result<bool> {
        let conn = &self.conn;
        let tab = self.tabs.iter_mut().find(|t| t.id == id).ok_or(CoreError::NoSuchTab(id))?;
        if !tab.engine.is_base_query() {
            return Err(CoreError::invalid("Cells can only be edited before any transform is applied"));
        }
        if row >= tab.engine.total_rows() {
            return Err(CoreError::invalid(format!("Row {row} is out of range")));
        }
        if tab.formulas.iter().any(|f| f.name == column) {
            return Err(CoreError::invalid(format!("\"{column}\" is a formula column: change its formula instead")));
        }
        let columns = describe(conn, &format!("SELECT * FROM {}", quote_ident(&tab.source_view())))?;
        let col = columns.iter().find(|c| c.name == column).ok_or_else(|| CoreError::UnknownColumn(column.into()))?;
        let is_text = col.dtype.eq_ignore_ascii_case("VARCHAR");
        let value = match value {
            Some(v) if v.is_empty() && !is_text => None,
            other => other,
        };
        validate_value(conn, &col.dtype, &value)?;

        let current: Cell = conn.query_row(
            &format!("SELECT CAST({} AS VARCHAR) FROM {} LIMIT 1 OFFSET {row}", quote_ident(column), quote_ident(&tab.name)),
            [],
            |r| r.get(0),
        )?;
        if current == value {
            return Ok(false);
        }
        let original: Cell = match tab.edits.get(row, column) {
            Some(e) => e.old_value.clone(),
            None => current,
        };
        let previous = tab.edits.get(row, column).cloned();
        let previous_for_undo = previous.clone();
        tab.edits.add(CellEdit { row, column: column.to_string(), old_value: original, new_value: value });
        if let Err(e) = rebuild_overlay(conn, tab) {
            tab.edits.remove(row, column);
            if let Some(p) = previous {
                tab.edits.add(p);
            }
            return Err(e);
        }
        let row_label = (row + 1).to_string();
        tab.steps.push(Step {
            label: StepLabel::new("step.cell_edit", &[("column", column), ("row", &row_label)]),
            // The cell's earlier pending edit, so undo goes back to it (not
            // to the file's value).
            kind: StepKind::CellEdits { previous: vec![(row, column.to_string(), previous_for_undo)] },
        });
        Ok(true)
    }

    /// Edit several cells as one step (a fill). Cells whose value does not
    /// change are skipped; returns how many changed. Same rules as
    /// [`Workspace::edit_cell`].
    pub fn edit_cells(&mut self, id: TabId, edits: &[(u64, String, Cell)]) -> Result<usize> {
        const MAX_CELLS: usize = 100_000;
        if edits.len() > MAX_CELLS {
            return Err(CoreError::invalid(format!("Edit at most {MAX_CELLS} cells at once")));
        }
        let conn = &self.conn;
        let tab = self.tabs.iter_mut().find(|t| t.id == id).ok_or(CoreError::NoSuchTab(id))?;
        if !tab.engine.is_base_query() {
            return Err(CoreError::invalid("Cells can only be edited before any transform is applied"));
        }
        let total = tab.engine.total_rows();
        let columns = describe(conn, &format!("SELECT * FROM {}", quote_ident(&tab.source_view())))?;
        let before = tab.edits.clone();
        let mut previous = Vec::new();
        for (row, column, value) in edits {
            if *row >= total {
                tab.edits = before;
                return Err(CoreError::invalid(format!("Row {row} is out of range")));
            }
            if tab.formulas.iter().any(|f| &f.name == column) {
                tab.edits = before;
                return Err(CoreError::invalid(format!("\"{column}\" is a formula column: change its formula instead")));
            }
            let Some(col) = columns.iter().find(|c| &c.name == column) else {
                tab.edits = before;
                return Err(CoreError::UnknownColumn(column.clone()));
            };
            let value = match value {
                Some(v) if v.is_empty() && !col.dtype.eq_ignore_ascii_case("VARCHAR") => None,
                other => other.clone(),
            };
            if let Err(e) = validate_value(conn, &col.dtype, &value) {
                tab.edits = before;
                return Err(e);
            }
            let current: Cell = conn.query_row(
                &format!("SELECT CAST({} AS VARCHAR) FROM {} LIMIT 1 OFFSET {row}", quote_ident(column), quote_ident(&tab.name)),
                [],
                |r| r.get(0),
            )?;
            if current == value {
                continue;
            }
            let prev = tab.edits.get(*row, column).cloned();
            let original = prev.as_ref().map(|e| e.old_value.clone()).unwrap_or(current);
            previous.push((*row, column.clone(), prev));
            tab.edits.add(CellEdit { row: *row, column: column.clone(), old_value: original, new_value: value });
        }
        if previous.is_empty() {
            return Ok(0);
        }
        if let Err(e) = rebuild_overlay(conn, tab) {
            tab.edits = before;
            return Err(e);
        }
        let n = previous.len();
        tab.steps.push(Step {
            label: StepLabel::new("step.cell_edits", &[("count", &n.to_string())]),
            kind: StepKind::CellEdits { previous },
        });
        Ok(n)
    }

    pub fn pending_edits(&self, id: TabId) -> Result<Vec<CellEdit>> {
        Ok(self.tab(id)?.edits.all().to_vec())
    }

    pub fn has_unsaved_changes(&self, id: TabId) -> Result<bool> {
        let t = self.tab(id)?;
        Ok(!t.steps.is_empty() || t.result_table.is_some())
    }

    /// Write the tab's current result (edits and transforms applied) to
    /// `target`, or back to its own file. The tab then reopens from the
    /// written file with no pending steps. Returns rows written.
    pub fn save(&mut self, id: TabId, target: Option<&Path>) -> Result<u64> {
        let (query, out, name, from, sheet) = {
            let t = self.tab(id)?;
            if target.is_none() && t.result_table.is_some() {
                return Err(CoreError::invalid("This table is not a file yet: use Save As"));
            }
            let out = target.map(Path::to_path_buf).unwrap_or_else(|| t.path.clone());
            (t.engine.current_query().to_string(), out, t.name.clone(), t.path.clone(), t.sheet.clone())
        };
        FileFormat::require(&out)?;
        let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if ext == "xls" {
            return Err(CoreError::invalid(
                "ParVu can't write the old .xls format: use Save As and choose an .xlsx file",
            ));
        }
        // A workbook is written whole: every sheet, with the sheet changes.
        if let (Some(own), "xlsx") = (sheet, ext.as_str()) {
            let key = book_key(&from);
            let plan = match self.workbooks.get(&key) {
                Some(p) => p.clone(),
                None => crate::workbook::WorkbookPlan::from_file(&from)?,
            };
            let rows = crate::workbook::write_workbook(&self.conn, &from, &out, &plan, &own, &query)?;
            let same_file = book_key(&out) == key;
            if same_file {
                // Other tabs of this workbook now show sheets with their new names.
                for t in self.tabs.iter_mut().filter(|t| t.id != id && book_key(&t.path) == key) {
                    if let Some(s) = t.sheet.as_deref().and_then(|s| plan.saved_name(s)) {
                        t.sheet = Some(s.to_string());
                    }
                }
                self.workbooks.remove(&key);
            }
            let new_name = plan.saved_name(&own).unwrap_or(&own).to_string();
            self.reload(id, &name, &out, Some(&new_name))?;
            return Ok(rows);
        }
        let rows = export(&self.conn, &query, &ExportOptions::for_path(&out))?;
        let sheet = if ext == "xlsx" { Some(crate::workbook::first_sheet(&out)?) } else { None };
        self.reload(id, &name, &out, sheet.as_deref())?;
        Ok(rows)
    }

    /// The sheets of the workbook tab `id` shows, with unsaved changes.
    pub fn sheets(&self, id: TabId) -> Result<SheetsView> {
        let t = self.tab(id)?;
        let Some(own) = t.sheet.as_deref() else {
            return Err(CoreError::invalid("This table is not an Excel workbook"));
        };
        let key = book_key(&t.path);
        let (plan, changed) = match self.workbooks.get(&key) {
            Some(p) => (p.clone(), true),
            None => (crate::workbook::WorkbookPlan::from_file(&t.path)?, false),
        };
        let open = self.tabs.iter().filter(|x| book_key(&x.path) == key).filter_map(|x| x.sheet.clone()).collect();
        Ok(SheetsView { current: plan.saved_name(own).unwrap_or(own).to_string(), sheets: plan.sheets, open, changed })
    }

    /// Rename, hide, show, delete, restore, move or duplicate a sheet of the
    /// workbook tab `id` shows. Saved with the workbook.
    pub fn sheet_op(&mut self, id: TabId, op: &crate::workbook::SheetOp) -> Result<SheetsView> {
        let path = self.tab(id)?.path.clone();
        if self.tab(id)?.sheet.is_none() {
            return Err(CoreError::invalid("This table is not an Excel workbook"));
        }
        let key = book_key(&path);
        let stored = crate::workbook::WorkbookPlan::from_file(&path)?;
        let mut plan = self.workbooks.get(&key).cloned().unwrap_or_else(|| stored.clone());
        let shown: Vec<String> = self.tabs.iter().filter(|t| book_key(&t.path) == key).filter_map(|t| t.sheet.clone()).collect();
        plan.apply(op, &shown)?;
        if plan.differs_from(&stored) {
            self.workbooks.insert(key, plan);
        } else {
            self.workbooks.remove(&key);
        }
        self.sheets(id)
    }

    /// Show another sheet of the workbook in tab `id` (by its stored name).
    /// The tab starts afresh: its steps, cell edits and formulas go.
    pub fn set_sheet(&mut self, id: TabId, sheet: &str) -> Result<()> {
        let (name, path) = {
            let t = self.tab(id)?;
            if t.sheet.is_none() {
                return Err(CoreError::invalid("This table is not an Excel workbook"));
            }
            (t.name.clone(), t.path.clone())
        };
        self.reload(id, &name, &path, Some(sheet))
    }

    /// Point the tab at `path` again from scratch (after a save).
    fn reload(&mut self, id: TabId, name: &str, path: &Path, sheet: Option<&str>) -> Result<()> {
        let src = source_view(name);
        // ParVu writes standard files, so the parsing options no longer apply.
        let source = register_source(&self.conn, &src, &excel_table(name), path, self.detect, &CsvOptions::default(), sheet)?;
        // The saved file holds the formulas' values as ordinary columns.
        create_views(&self.conn, name, &[], &[])?;
        let engine = QueryEngine::new(&self.conn, name, self.page_size)?;
        let tab = self.tab_mut(id)?;
        tab.engine = engine;
        tab.path = path.to_path_buf();
        tab.format = FileFormat::require(path)?;
        tab.edits.clear();
        tab.steps.clear();
        tab.source = source;
        tab.csv = CsvOptions::default();
        tab.formulas.clear();
        tab.sheet = sheet.map(str::to_string);
        // A saved result is a file now.
        tab.origin = None;
        if let Some(table) = tab.result_table.take() {
            let _ = self.conn.execute_batch(&format!("DROP TABLE IF EXISTS {}", quote_ident(&table)));
        }
        Ok(())
    }

    /// Export the tab's current result with full options. Runs on this
    /// connection; for background work use [`Workspace::clone_connection`]
    /// with [`crate::export::export`] and the query from [`Workspace::current_query`].
    pub fn export(&self, id: TabId, options: &ExportOptions) -> Result<u64> {
        export(&self.conn, self.tab(id)?.engine.current_query(), options)
    }

    pub fn column_type(&self, id: TabId, column: &str) -> Result<String> {
        self.columns(id)?
            .into_iter()
            .find(|c| c.name == column)
            .map(|c| c.dtype)
            .ok_or_else(|| CoreError::UnknownColumn(column.into()))
    }

    pub fn column_stats(&self, id: TabId, column: &str) -> Result<ColumnStats> {
        let (source, dtype, approx) = self.stats_input(id, column)?;
        column_stats(&self.conn, &source, column, &dtype, approx)
    }

    /// What [`crate::stats::column_stats`] needs: the unordered query, the
    /// column's type, and whether to estimate the distinct count.
    pub fn stats_input(&self, id: TabId, column: &str) -> Result<(String, String, bool)> {
        let dtype = self.column_type(id, column)?;
        let approx = self.tab(id)?.engine.total_rows() > crate::stats::EXACT_DISTINCT_ROWS;
        Ok((self.unordered_query(id)?, dtype, approx))
    }

    /// The tab's current query and columns, for chart data on any connection.
    pub fn chart_input(&self, id: TabId) -> Result<(String, Vec<ColumnMeta>)> {
        let t = self.tab(id)?;
        Ok((t.engine.current_query().to_string(), t.engine.columns(&self.conn)?))
    }

    /// Chart data for the tab's current result.
    pub fn chart(&self, id: TabId, spec: &crate::chart::ChartSpec) -> Result<crate::chart::ChartData> {
        let (source, columns) = self.chart_input(id)?;
        crate::chart::chart_data(&self.conn, &source, &columns, spec)
    }

    /// Minimum and maximum of columns read as numbers (for colour scales),
    /// `None` for a column without numeric values; one scan for all.
    pub fn numeric_ranges(&self, id: TabId, columns: &[String]) -> Result<Vec<Option<(f64, f64)>>> {
        for c in columns {
            self.column_type(id, c)?;
        }
        crate::stats::numeric_ranges(&self.conn, &self.unordered_query(id)?, columns)
    }

    /// The tab's query without a sort at the end, for work that doesn't
    /// depend on row order (unique values, statistics, ranges): sorting a
    /// big table first would only make it slower.
    pub fn unordered_query(&self, id: TabId) -> Result<String> {
        let t = self.tab(id)?;
        let last_is_sort = t.steps.last().is_some_and(|s| {
            s.kind == StepKind::Sql && (s.label.key == "step.sort" || s.label.key == "step.sort_by")
        });
        Ok(match (last_is_sort, t.engine.previous_query()) {
            (true, Some(before)) => before.to_string(),
            _ => t.engine.current_query().to_string(),
        })
    }

    /// Unique values of a column (see [`crate::stats::unique_values`]), on
    /// this workspace's connection. The app runs it on a cloned connection.
    pub fn unique_values(&self, id: TabId, column: &str) -> Result<crate::stats::UniqueValues> {
        self.column_type(id, column)?;
        crate::stats::unique_values(&self.conn, &self.unordered_query(id)?, column, &crate::job::Cancel::new(), &mut |_, _| {})
    }

    /// Column values formatted as a Python-style tuple, for "copy as tuple".
    pub fn column_tuple(&self, id: TabId, column: &str, mode: SampleMode, n: u64) -> Result<String> {
        self.column_type(id, column)?;
        let values = column_sample(&self.conn, self.tab(id)?.engine.current_query(), column, mode, n)?;
        Ok(crate::stats::python_tuple(&values))
    }

    pub fn find(&self, id: TabId, options: &SearchOptions) -> Result<Option<SearchHit>> {
        find_next(&self.conn, self.tab(id)?.engine.current_query(), options)
    }
}

/// Recreate the tab's user-facing view from its source plus pending edits.
fn rebuild_overlay(conn: &Connection, tab: &mut Tab) -> Result<()> {
    create_views(conn, &tab.name, tab.edits.all(), &tab.formulas)?;
    tab.engine.refresh(conn)
}

/// Run a [`Workspace::json_keys_query`].
pub fn json_keys_from(conn: &Connection, sql: &str) -> Result<Vec<JsonKey>> {
    let structure: Option<String> = conn.query_row(sql, [], |r| r.get(0))?;
    Ok(structure.map(|s| JsonKey::from_structure(&s)).unwrap_or_default())
}

/// Run a [`Workspace::conversion_error_query`]: `(errors, checked)`.
pub fn conversion_errors(conn: &Connection, sql: &str) -> Result<(u64, u64)> {
    Ok(conn.query_row(sql, [], |r| Ok((r.get::<_, i64>(0)?.max(0) as u64, r.get::<_, i64>(1)?.max(0) as u64)))?)
}
