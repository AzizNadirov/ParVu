//! Query state for one tab (port of `core/query_engine.py`, shared-connection
//! mode, which is the mode the app always used).
//!
//! The engine never materialises data. It keeps the current SQL text, a
//! history of previous queries for undo, and the row count for paging. Every
//! call takes the DuckDB connection, which the [`crate::Workspace`] owns.

use duckdb::Connection;
use duckdb::arrow::record_batch::RecordBatch;

use crate::error::{CoreError, Result};
use crate::page::{Cell, ColumnMeta, format_batches};
use crate::pagination::Paginator;
use crate::sql::{quote_ident, wrap};

/// A fetched page: `(page, offset, columns, rows)`.
pub type Fetched = (u64, u64, Vec<ColumnMeta>, Vec<Vec<Cell>>);

#[derive(Debug, Clone)]
pub struct QueryEngine {
    relation: String,
    base_query: String,
    current: String,
    history: Vec<String>,
    paginator: Paginator,
    /// Columns of `current`, cached because every page fetch needs them.
    /// Cleared whenever the query or the underlying view changes.
    columns: std::cell::RefCell<Option<Vec<ColumnMeta>>>,
}

/// Fetch every batch of `sql`.
pub(crate) fn query_batches(conn: &Connection, sql: &str) -> Result<Vec<RecordBatch>> {
    let mut stmt = conn.prepare(sql)?;
    Ok(stmt.query_arrow([])?.collect())
}

/// Bind and run `query` with `LIMIT 0` so errors surface before any state changes.
pub(crate) fn validate(conn: &Connection, query: &str) -> Result<()> {
    query_batches(conn, &format!("{} LIMIT 0", wrap(query))).map(|_| ())
}

pub(crate) fn count_rows(conn: &Connection, query: &str) -> Result<u64> {
    let n: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM ({query})"), [], |r| r.get(0))?;
    Ok(n.max(0) as u64)
}

/// Column names and DuckDB types of `query`, without running it.
pub fn describe(conn: &Connection, query: &str) -> Result<Vec<ColumnMeta>> {
    let mut stmt = conn.prepare(&format!("DESCRIBE {}", wrap(query)))?;
    let rows = stmt.query_map([], |r| Ok(ColumnMeta::new(r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

impl QueryEngine {
    /// Engine over the view `relation`, which must already exist.
    pub fn new(conn: &Connection, relation: &str, page_size: u64) -> Result<Self> {
        let base_query = format!("SELECT * FROM {}", quote_ident(relation));
        validate(conn, &base_query)?;
        let total = count_rows(conn, &base_query)?;
        Ok(Self {
            relation: relation.to_string(),
            current: base_query.clone(),
            base_query,
            history: Vec::new(),
            paginator: Paginator::new(total, page_size),
            columns: std::cell::RefCell::new(None),
        })
    }

    pub fn relation(&self) -> &str {
        &self.relation
    }
    pub fn base_query(&self) -> &str {
        &self.base_query
    }
    pub fn current_query(&self) -> &str {
        &self.current
    }
    pub fn is_base_query(&self) -> bool {
        self.current == self.base_query
    }
    pub fn can_undo(&self) -> bool {
        !self.history.is_empty()
    }
    /// The query before the most recent change, if any.
    pub fn previous_query(&self) -> Option<&str> {
        self.history.last().map(String::as_str)
    }
    pub fn total_rows(&self) -> u64 {
        self.paginator.total_rows()
    }
    pub fn total_pages(&self) -> u64 {
        self.paginator.total_pages()
    }
    pub fn page_size(&self) -> u64 {
        self.paginator.page_size()
    }
    pub fn paginator(&self) -> &Paginator {
        &self.paginator
    }

    pub fn set_page_size(&mut self, page_size: u64) {
        self.paginator = Paginator::new(self.paginator.total_rows(), page_size);
    }

    pub fn columns(&self, conn: &Connection) -> Result<Vec<ColumnMeta>> {
        if let Some(cols) = self.columns.borrow().as_ref() {
            return Ok(cols.clone());
        }
        let cols = describe(conn, &self.current)?;
        *self.columns.borrow_mut() = Some(cols.clone());
        Ok(cols)
    }

    fn invalidate_columns(&self) {
        self.columns.borrow_mut().take();
    }

    /// Replace the current query (user SQL or a transform). On error nothing changes.
    pub fn set_query(&mut self, conn: &Connection, query: &str) -> Result<()> {
        let query = query.trim().trim_end_matches(';').trim();
        if query.is_empty() {
            return Err(CoreError::invalid("Query is empty"));
        }
        validate(conn, query)?;
        let total = count_rows(conn, query)?;
        self.history.push(std::mem::replace(&mut self.current, query.to_string()));
        self.paginator.recalculate(total);
        self.invalidate_columns();
        Ok(())
    }

    /// Restore the previous query.
    pub fn undo(&mut self, conn: &Connection) -> Result<()> {
        let previous = self.history.last().ok_or(CoreError::NothingToUndo)?.clone();
        validate(conn, &previous)?;
        let total = count_rows(conn, &previous)?;
        self.history.pop();
        self.current = previous;
        self.paginator.recalculate(total);
        self.invalidate_columns();
        Ok(())
    }

    /// Back to the unmodified file view, clearing history.
    pub fn reset(&mut self, conn: &Connection) -> Result<()> {
        self.current = self.base_query.clone();
        self.history.clear();
        self.refresh(conn)
    }

    /// Recount rows, e.g. after the underlying view changed.
    pub fn refresh(&mut self, conn: &Connection) -> Result<()> {
        self.invalidate_columns();
        let total = count_rows(conn, &self.current)?;
        self.paginator.recalculate(total);
        Ok(())
    }

    /// Rows of `page` (1-based, clamped). Returns `(page, offset, columns, rows)`.
    pub fn fetch(&self, conn: &Connection, page: u64) -> Result<Fetched> {
        let page = self.paginator.clamp_page(page);
        let offset = self.paginator.page_offset(page);
        let columns = self.columns(conn)?;
        let sql = format!("{} LIMIT {} OFFSET {offset}", wrap(&self.current), self.page_size());
        let rows = format_batches(&query_batches(conn, &sql)?)?;
        Ok((page, offset, columns, rows))
    }
}
