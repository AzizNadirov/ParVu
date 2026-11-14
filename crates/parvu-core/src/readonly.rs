//! Read-only access to the open tabs, for the AI assistant and the MCP
//! server (plans/AI_PLAN.md §3, §6).
//!
//! Queries are parsed with DuckDB's own parser (`json_serialize_sql`), which
//! returns a tree only for SELECT statements. The guard then walks the tree:
//! every table must be an open tab (or a CTE of the same query), table
//! functions such as `read_csv` / `read_text` are refused, and so are file
//! paths used as tables. That keeps the caller to the data the user opened.
//! Tab names are rewritten to per-session views of each tab's *current*
//! result, so queries see the tab with its applied steps, as in expression
//! mode.
//!
//! A [`ReadSession`] runs on its own DuckDB connection, so it never needs the
//! workspace once created and can be interrupted.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use duckdb::{Connection, InterruptHandle};
use serde::Serialize;
use serde_json::Value;

use crate::engine::{count_rows, describe, query_batches};
use crate::error::{CoreError, Result};
use crate::ops::Operation;
use crate::page::{Cell, ColumnMeta, format_batches};
use crate::sql::quote_ident;
use crate::stats::{ColumnStats, column_stats};

/// Scalar functions that reveal things outside the data.
const DENIED_FUNCTIONS: &[&str] = &["current_setting", "getenv"];

/// Table functions that only generate rows and read nothing.
const ALLOWED_TABLE_FUNCTIONS: &[&str] = &["range", "generate_series", "unnest"];

/// Why a query was refused. The message is meant for the model or user.
fn rejected(msg: impl Into<String>) -> CoreError {
    CoreError::invalid(msg)
}

/// Check that `sql` is one read-only SELECT over known tables and return it
/// with every table reference renamed by `resolve` (tab name → view name).
/// `resolve` returns `None` for names that are not open tabs.
pub fn guard_select(conn: &Connection, sql: &str, resolve: &dyn Fn(&str) -> Option<String>) -> Result<String> {
    let json: String = conn.query_row("SELECT json_serialize_sql(?)", [sql], |r| r.get(0))?;
    let mut tree: Value = serde_json::from_str(&json).map_err(|e| rejected(format!("could not read the query: {e}")))?;
    if tree["error"].as_bool() == Some(true) {
        let msg = tree["error_message"].as_str().unwrap_or("invalid query");
        return Err(if msg.contains("Only SELECT") {
            rejected("Only SELECT queries are allowed.")
        } else {
            rejected(msg.to_string())
        });
    }
    let statements = tree["statements"].as_array().map_or(0, Vec::len);
    if statements != 1 {
        return Err(rejected("Exactly one SELECT statement is allowed."));
    }

    let mut ctes = HashSet::new();
    collect_ctes(&tree, &mut ctes);
    for cte in &ctes {
        if resolve(cte).is_some() {
            return Err(rejected(format!("A CTE may not reuse the table name \"{cte}\".")));
        }
    }
    rewrite(&mut tree, &ctes, resolve)?;

    let out = serde_json::to_string(&tree).map_err(|e| rejected(e.to_string()))?;
    let back: String = conn.query_row("SELECT json_deserialize_sql(?::JSON)", [out], |r| r.get(0))?;
    Ok(back)
}

/// Lower-cased names of all CTEs defined anywhere in the tree.
fn collect_ctes(v: &Value, out: &mut HashSet<String>) {
    match v {
        Value::Object(map) => {
            if let Some(entries) = map.get("cte_map").and_then(|m| m["map"].as_array()) {
                for e in entries {
                    if let Some(k) = e["key"].as_str() {
                        out.insert(k.to_lowercase());
                    }
                }
            }
            map.values().for_each(|c| collect_ctes(c, out));
        }
        Value::Array(items) => items.iter().for_each(|c| collect_ctes(c, out)),
        _ => {}
    }
}

fn rewrite(v: &mut Value, ctes: &HashSet<String>, resolve: &dyn Fn(&str) -> Option<String>) -> Result<()> {
    match v {
        Value::Object(map) => {
            match map.get("type").and_then(Value::as_str) {
                Some("TABLE_FUNCTION") => {
                    let name = map
                        .get("function")
                        .and_then(|f| f["function_name"].as_str())
                        .unwrap_or("?")
                        .to_string();
                    if !ALLOWED_TABLE_FUNCTIONS.contains(&name.to_lowercase().as_str()) {
                        return Err(rejected(format!(
                            "Table function {name}() is not allowed; query the open tables by name."
                        )));
                    }
                }
                Some("SHOW_REF") => return Err(rejected("DESCRIBE / SHOW is not allowed here.")),
                Some("BASE_TABLE") => {
                    let name = map.get("table_name").and_then(Value::as_str).unwrap_or("").to_string();
                    let schema = map.get("schema_name").and_then(Value::as_str).unwrap_or("");
                    let catalog = map.get("catalog_name").and_then(Value::as_str).unwrap_or("");
                    if !schema.is_empty() || !catalog.is_empty() {
                        return Err(rejected(format!("Unknown table \"{catalog}.{schema}.{name}\".")));
                    }
                    if !ctes.contains(&name.to_lowercase()) {
                        let Some(view) = resolve(&name) else {
                            return Err(rejected(format!("Unknown table \"{name}\".")));
                        };
                        map.insert("table_name".into(), Value::String(view.clone()));
                        map.insert("qualified_name".into(), serde_json::json!({ "path": [view] }));
                        // Keep the name the query used, so `sales.price` still resolves.
                        if map.get("alias").and_then(Value::as_str).unwrap_or("").is_empty() {
                            map.insert("alias".into(), Value::String(name));
                        }
                    }
                }
                _ => {}
            }
            if map.get("class").and_then(Value::as_str) == Some("FUNCTION")
                && let Some(f) = map.get("function_name").and_then(Value::as_str)
                && DENIED_FUNCTIONS.contains(&f.to_lowercase().as_str())
            {
                return Err(rejected(format!("Function {f}() is not allowed.")));
            }
            for child in map.values_mut() {
                rewrite(child, ctes, resolve)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                rewrite(child, ctes, resolve)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Rows of a read-only query.
#[derive(Debug, Clone, Serialize)]
pub struct QueryRows {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<Cell>>,
    /// More rows existed than were returned.
    pub truncated: bool,
}

/// Name, size and columns of one open table.
#[derive(Debug, Clone, Serialize)]
pub struct TableInfo {
    pub name: String,
    pub rows: u64,
    pub columns: Vec<ColumnMeta>,
    /// Labels of the applied steps (step keys with their arguments).
    pub steps: Vec<crate::ops::StepLabel>,
}

pub(crate) struct SessionTable {
    pub name: String,
    pub query: String,
    pub steps: Vec<crate::ops::StepLabel>,
}

/// A snapshot of the open tabs on its own connection. Tab names resolve to
/// each tab's current result at the time the session was created.
pub struct ReadSession {
    conn: Connection,
    /// lower-cased tab name → (tab name, view name, steps)
    tables: HashMap<String, (String, String, Vec<crate::ops::StepLabel>)>,
    order: Vec<String>,
}

impl ReadSession {
    pub(crate) fn new(conn: Connection, tables: Vec<SessionTable>) -> Result<Self> {
        let mut map = HashMap::new();
        let mut order = Vec::new();
        for (i, t) in tables.into_iter().enumerate() {
            let view = format!("__pv_ro_{i}");
            conn.execute_batch(&format!("CREATE OR REPLACE TEMP VIEW {} AS {}", quote_ident(&view), t.query))?;
            order.push(t.name.clone());
            map.insert(t.name.to_lowercase(), (t.name, view, t.steps));
        }
        Ok(Self { conn, tables: map, order })
    }

    /// Stops the query running on this session (from another thread).
    pub fn interrupt_handle(&self) -> Arc<InterruptHandle> {
        self.conn.interrupt_handle()
    }

    /// Open table names, in tab order.
    pub fn tables(&self) -> &[String] {
        &self.order
    }

    fn unknown(&self, table: &str) -> CoreError {
        rejected(format!("Unknown table \"{table}\". Open tables: {}.", self.order.join(", ")))
    }

    fn view(&self, table: &str) -> Result<&str> {
        self.tables.get(&table.to_lowercase()).map(|(_, v, _)| v.as_str()).ok_or_else(|| self.unknown(table))
    }

    /// Check a SELECT and rewrite it onto this session's views.
    pub fn guard(&self, sql: &str) -> Result<String> {
        guard_select(&self.conn, sql, &|name| self.view(name).ok().map(str::to_string)).map_err(|e| match e {
            CoreError::Invalid(msg) if msg.starts_with("Unknown table") => {
                rejected(format!("{msg} Open tables: {}.", self.order.join(", ")))
            }
            other => other,
        })
    }

    /// Run a guarded SELECT, returning at most `max_rows` rows. With
    /// `timeout`, the query is interrupted when it runs longer.
    pub fn query(&self, sql: &str, max_rows: usize, timeout: Option<Duration>) -> Result<QueryRows> {
        let safe = self.guard(sql)?;
        self.run(&safe, max_rows, timeout)
    }

    fn run(&self, safe_sql: &str, max_rows: usize, timeout: Option<Duration>) -> Result<QueryRows> {
        let limited = format!("SELECT * FROM ({safe_sql}) LIMIT {}", max_rows + 1);
        let (done, watchdog) = match timeout {
            Some(t) => {
                let (tx, rx) = mpsc::channel::<()>();
                let handle = self.interrupt_handle();
                let w = std::thread::spawn(move || {
                    if rx.recv_timeout(t) == Err(mpsc::RecvTimeoutError::Timeout) {
                        handle.interrupt();
                        true
                    } else {
                        false
                    }
                });
                (Some(tx), Some(w))
            }
            None => (None, None),
        };
        let result = describe(&self.conn, &limited)
            .and_then(|cols| Ok((cols, format_batches(&query_batches(&self.conn, &limited)?)?)));
        drop(done);
        let timed_out = watchdog.is_some_and(|w| w.join().unwrap_or(false));
        let (columns, mut rows) = match result {
            Err(_) if timed_out => return Err(rejected("The query took too long and was stopped.")),
            other => other?,
        };
        let truncated = rows.len() > max_rows;
        rows.truncate(max_rows);
        Ok(QueryRows { columns, rows, truncated })
    }

    /// Size, columns and steps of one table.
    pub fn describe_table(&self, table: &str) -> Result<TableInfo> {
        let (name, view, steps) = self.tables.get(&table.to_lowercase()).ok_or_else(|| self.unknown(table))?;
        let from = format!("SELECT * FROM {}", quote_ident(view));
        Ok(TableInfo {
            name: name.clone(),
            rows: count_rows(&self.conn, &from)?,
            columns: describe(&self.conn, &from)?,
            steps: steps.clone(),
        })
    }

    /// First, last or random rows of a table.
    pub fn sample(&self, table: &str, mode: SampleRows, n: usize) -> Result<QueryRows> {
        let v = quote_ident(self.view(table)?);
        let sql = match mode {
            SampleRows::Head => format!("SELECT * FROM {v}"),
            SampleRows::Tail => format!(
                "SELECT * EXCLUDE (__pv_n) FROM (SELECT *, ROW_NUMBER() OVER () AS __pv_n FROM {v}) \
                 QUALIFY __pv_n > COUNT(*) OVER () - {n} ORDER BY __pv_n"
            ),
            SampleRows::Random => format!("SELECT * FROM {v} USING SAMPLE reservoir({n} ROWS) REPEATABLE (42)"),
        };
        self.run(&sql, n, Some(Duration::from_secs(20)))
    }

    /// Statistics for one column (the column statistics dialog's numbers).
    pub fn column_stats(&self, table: &str, column: &str) -> Result<ColumnStats> {
        let info = self.describe_table(table)?;
        let col = info
            .columns
            .iter()
            .find(|c| c.name == column)
            .ok_or_else(|| CoreError::UnknownColumn(column.to_string()))?;
        column_stats(&self.conn, &format!("SELECT * FROM {}", quote_ident(self.view(table)?)), column, &col.dtype)
    }

    /// The `k` most frequent values of a column, with counts.
    pub fn value_counts(&self, table: &str, column: &str, k: usize) -> Result<QueryRows> {
        let v = quote_ident(self.view(table)?);
        let c = quote_ident(column);
        let sql = format!("SELECT {c} AS value, COUNT(*) AS count FROM {v} GROUP BY ALL ORDER BY count DESC, value LIMIT {k}");
        self.run(&sql, k, Some(Duration::from_secs(20)))
    }

    /// Check the raw SQL an operation carries (expressions in `AddColumn` and
    /// `Replace`) against the same rules as queries, so a proposed change
    /// cannot read other files either.
    pub fn check_operation(&self, table: &str, op: &Operation) -> Result<()> {
        self.view(table)?;
        let expression = match op {
            Operation::AddColumn { expression, .. } | Operation::Replace { expression, .. } => expression,
            Operation::Join { right, .. } => return self.view(right).map(|_| ()),
            Operation::Append { tables, .. } => return tables.iter().try_for_each(|t| self.view(t).map(|_| ())),
            _ => return Ok(()),
        };
        // `"table"."col"` references must work, as in Workspace::apply.
        let sql = format!("SELECT {expression} FROM {} AS {}", quote_ident(table), quote_ident(table));
        self.guard(&sql).map(|_| ())
    }
}

/// Which rows [`ReadSession::sample`] returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SampleRows {
    Head,
    Tail,
    Random,
}
