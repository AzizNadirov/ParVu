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
use crate::ops::{BuiltOp, OpContext, Operation, StepLabel};
use crate::page::{Cell, ColumnMeta, Page};
use crate::search::{SearchHit, SearchOptions, find_next};
use crate::sql::{quote_ident, slugify_name};
use crate::stats::{ColumnStats, SampleMode, column_sample, column_stats, unique_values};

pub type TabId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
enum StepKind {
    Sql,
    CellEdit { row: u64, column: String },
    /// Several cells edited at once (fill down); keeps each cell's previous
    /// pending edit so undo restores it exactly.
    CellEdits { previous: Vec<(u64, String, Option<CellEdit>)> },
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
}

impl Tab {
    fn source_view(&self) -> String {
        source_view(&self.name)
    }
}

fn source_view(name: &str) -> String {
    format!("__pv_src_{name}")
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
}

/// Default cap on DuckDB's working memory. Larger sorts, joins and
/// aggregations spill to the temp directory instead of growing the process.
pub const DEFAULT_MEMORY_LIMIT_MB: u64 = 1024;

/// Most threads DuckDB may use. More threads speed up large scans only a
/// little but each one holds its own buffers.
pub const MAX_THREADS: usize = 8;

/// DuckDB settings that keep memory use bounded and predictable.
///
/// DuckDB's own defaults (80% of RAM, one thread per core) suit a database
/// server; a desktop viewer shares the machine with other programs. The
/// memory limit can be changed with the `PARVU_MEMORY_LIMIT_MB` environment
/// variable.
fn resource_settings() -> String {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, MAX_THREADS);
    let limit = std::env::var("PARVU_MEMORY_LIMIT_MB")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v >= 128)
        .unwrap_or(DEFAULT_MEMORY_LIMIT_MB);
    // Return freed memory to the system quickly instead of keeping it
    // cached in the allocator after a large query.
    format!(
        "SET threads = {threads}; SET memory_limit = '{limit}MB'; \
         SET allocator_flush_threshold = '32MB'; SET allocator_background_threads = true;"
    )
}

pub struct Workspace {
    conn: Connection,
    tabs: Vec<Tab>,
    next_id: TabId,
    page_size: u64,
}

impl Workspace {
    /// New in-memory workspace. `temp_dir` is where DuckDB spills when a query
    /// needs more memory than is available.
    pub fn new(page_size: u64, temp_dir: Option<&Path>) -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(&resource_settings())?;
        if let Some(dir) = temp_dir {
            std::fs::create_dir_all(dir)?;
            conn.execute_batch(&format!(
                "SET temp_directory = {}",
                crate::sql::quote_literal(&dir.to_string_lossy())
            ))?;
        }
        Ok(Self { conn, tabs: Vec::new(), next_id: 1, page_size: page_size.max(1) })
    }

    /// Change DuckDB's working-memory limit (MB, at least 128).
    pub fn set_memory_limit_mb(&self, mb: u64) -> Result<()> {
        self.conn.execute_batch(&format!("SET memory_limit = '{}MB'", mb.max(128)))?;
        Ok(())
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// A second connection to the same database, for work on another thread
    /// (exports, long statistics) while the UI keeps paging.
    pub fn clone_connection(&self) -> Result<Connection> {
        Ok(self.conn.try_clone()?)
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
        let path = path.to_path_buf();
        if !path.is_file() {
            return Err(CoreError::FileNotFound(path.display().to_string()));
        }
        let format = FileFormat::require(&path)?;
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("data");
        let base = slugify_name(stem);
        let mut name = base.clone();
        let mut n = 2;
        while self.tabs.iter().any(|t| t.name == name) {
            name = format!("{base}_{n}");
            n += 1;
        }

        let src = source_view(&name);
        register_source(&self.conn, &src, &excel_table(&name), &path)?;
        let result = (|| {
            self.conn.execute_batch(&format!(
                "CREATE OR REPLACE VIEW {} AS SELECT * FROM {}",
                quote_ident(&name),
                quote_ident(&src)
            ))?;
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
        self.tabs.push(Tab { id, name, path, format, engine, edits: EditQueue::new(), steps: Vec::new() });
        Ok(id)
    }

    fn drop_relations(&self, name: &str) {
        let _ = self.conn.execute_batch(&format!(
            "DROP VIEW IF EXISTS {}; DROP VIEW IF EXISTS {}; DROP TABLE IF EXISTS {};",
            quote_ident(name),
            quote_ident(&source_view(name)),
            quote_ident(&excel_table(name))
        ));
    }

    pub fn close(&mut self, id: TabId) -> Result<()> {
        let idx = self.tabs.iter().position(|t| t.id == id).ok_or(CoreError::NoSuchTab(id))?;
        let tab = self.tabs.remove(idx);
        self.drop_relations(&tab.name);
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
            StepKind::CellEdit { row, column } => {
                let removed = tab.edits.remove(*row, column);
                if let Err(e) = rebuild_overlay(conn, tab) {
                    if let Some(r) = removed {
                        tab.edits.add(r);
                    }
                    return Err(e);
                }
            }
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
        }
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
            kind: StepKind::CellEdit { row, column: column.to_string() },
        });
        Ok(true)
    }

    /// Edit several cells as one step (fill down). Cells whose value does not
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
        Ok(!self.tab(id)?.steps.is_empty())
    }

    /// Write the tab's current result (edits and transforms applied) to
    /// `target`, or back to its own file. The tab then reopens from the
    /// written file with no pending steps. Returns rows written.
    pub fn save(&mut self, id: TabId, target: Option<&Path>) -> Result<u64> {
        let (query, out, name) = {
            let t = self.tab(id)?;
            let out = target.map(Path::to_path_buf).unwrap_or_else(|| t.path.clone());
            (t.engine.current_query().to_string(), out, t.name.clone())
        };
        FileFormat::require(&out)?;
        let rows = export(&self.conn, &query, &ExportOptions::for_path(&out))?;
        self.reload(id, &name, &out)?;
        Ok(rows)
    }

    /// Point the tab at `path` again from scratch (after a save).
    fn reload(&mut self, id: TabId, name: &str, path: &Path) -> Result<()> {
        let src = source_view(name);
        register_source(&self.conn, &src, &excel_table(name), path)?;
        self.conn.execute_batch(&format!(
            "CREATE OR REPLACE VIEW {} AS SELECT * FROM {}",
            quote_ident(name),
            quote_ident(&src)
        ))?;
        let engine = QueryEngine::new(&self.conn, name, self.page_size)?;
        let tab = self.tab_mut(id)?;
        tab.engine = engine;
        tab.path = path.to_path_buf();
        tab.format = FileFormat::require(path)?;
        tab.edits.clear();
        tab.steps.clear();
        Ok(())
    }

    /// Export the tab's current result with full options. Runs on this
    /// connection; for background work use [`Workspace::clone_connection`]
    /// with [`crate::export::export`] and the query from [`Workspace::current_query`].
    pub fn export(&self, id: TabId, options: &ExportOptions) -> Result<u64> {
        export(&self.conn, self.tab(id)?.engine.current_query(), options)
    }

    fn column_type(&self, id: TabId, column: &str) -> Result<String> {
        self.columns(id)?
            .into_iter()
            .find(|c| c.name == column)
            .map(|c| c.dtype)
            .ok_or_else(|| CoreError::UnknownColumn(column.into()))
    }

    pub fn column_stats(&self, id: TabId, column: &str) -> Result<ColumnStats> {
        let dtype = self.column_type(id, column)?;
        column_stats(&self.conn, self.tab(id)?.engine.current_query(), column, &dtype)
    }

    /// Chart data for the tab's current result.
    pub fn chart(&self, id: TabId, spec: &crate::chart::ChartSpec) -> Result<crate::chart::ChartData> {
        let t = self.tab(id)?;
        let columns = t.engine.columns(&self.conn)?;
        crate::chart::chart_data(&self.conn, t.engine.current_query(), &columns, spec)
    }

    /// Minimum and maximum of a column read as numbers (for colour scales),
    /// or `None` when it has no numeric values.
    pub fn numeric_range(&self, id: TabId, column: &str) -> Result<Option<(f64, f64)>> {
        self.column_type(id, column)?;
        let c = quote_ident(column);
        let (lo, hi): (Option<f64>, Option<f64>) = self.conn.query_row(
            &format!(
                "SELECT MIN(TRY_CAST({c} AS DOUBLE)), MAX(TRY_CAST({c} AS DOUBLE)) FROM ({})",
                self.tab(id)?.engine.current_query()
            ),
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(lo.zip(hi))
    }

    pub fn unique_values(&self, id: TabId, column: &str) -> Result<Vec<Cell>> {
        self.column_type(id, column)?;
        unique_values(&self.conn, self.tab(id)?.engine.current_query(), column)
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
    let src = tab.source_view();
    let columns = describe(conn, &format!("SELECT * FROM {}", quote_ident(&src)))?;
    let sql = overlay_sql(&src, &columns, tab.edits.all())?;
    conn.execute_batch(&format!("CREATE OR REPLACE VIEW {} AS {sql}", quote_ident(&tab.name)))?;
    tab.engine.refresh(conn)
}
