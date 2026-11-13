//! Full-table search (port of `QueryEngine.find_next_match`).

use duckdb::Connection;
use serde::{Deserialize, Serialize};

use crate::engine::describe;
use crate::error::Result;
use crate::sql::{quote_ident, quote_literal};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    #[default]
    Next,
    Prev,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchOptions {
    pub query: String,
    /// Restrict to one column; `None` searches all columns.
    pub column: Option<String>,
    /// Anchor: matches are strictly after (or before) this cell.
    pub after_row: Option<u64>,
    pub after_col: Option<u64>,
    pub direction: Direction,
    pub case_sensitive: bool,
    pub whole_cell: bool,
    pub regex: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    pub row: u64,
    pub column_index: u64,
    pub column: String,
    pub value: String,
}

fn predicate(col: &str, needle: &str, o: &SearchOptions) -> String {
    let col_expr = format!("CAST({} AS VARCHAR)", quote_ident(col));
    let lit = quote_literal(needle);
    if o.regex {
        let func = if o.whole_cell { "regexp_full_match" } else { "regexp_matches" };
        let flag = if o.case_sensitive { "" } else { ", 'i'" };
        return format!("{func}({col_expr}, {lit}{flag})");
    }
    if o.whole_cell {
        return if o.case_sensitive {
            format!("{col_expr} = {lit}")
        } else {
            format!("LOWER({col_expr}) = LOWER({lit})")
        };
    }
    let escaped = needle.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    let op = if o.case_sensitive { "LIKE" } else { "ILIKE" };
    format!("{col_expr} {op} {} ESCAPE '\\'", quote_literal(&format!("%{escaped}%")))
}

/// Build the search SQL. `None` when nothing can match.
pub fn build_search_sql(source: &str, columns: &[String], o: &SearchOptions) -> Option<String> {
    if o.query.is_empty() || columns.is_empty() {
        return None;
    }
    let scope: Vec<(usize, &String)> = match &o.column {
        Some(c) => vec![(columns.iter().position(|x| x == c)?, c)],
        None => columns.iter().enumerate().collect(),
    };
    let parts: Vec<String> = scope
        .iter()
        .map(|(idx, c)| {
            format!(
                "SELECT __rn, {idx} AS __col_idx, {} AS __col_name, CAST({} AS VARCHAR) AS __val FROM base WHERE {}",
                quote_literal(c),
                quote_ident(c),
                predicate(c, &o.query, o)
            )
        })
        .collect();
    let (cmp, order) = match o.direction {
        Direction::Next => (">", "ASC"),
        Direction::Prev => ("<", "DESC"),
    };
    let anchor = match (o.after_row, o.after_col) {
        (None, _) => String::new(),
        (Some(r), None) => format!("WHERE __rn {cmp} {r}"),
        (Some(r), Some(c)) => format!("WHERE (__rn {cmp} {r} OR (__rn = {r} AND __col_idx {cmp} {c}))"),
    };
    Some(format!(
        "WITH base AS (SELECT *, ROW_NUMBER() OVER () - 1 AS __rn FROM ({source})), hits AS ({}) \
         SELECT __rn, __col_idx, __col_name, __val FROM hits {anchor} ORDER BY __rn {order}, __col_idx {order} LIMIT 1",
        parts.join(" UNION ALL ")
    ))
}

/// Find the next (or previous) matching cell of `source`.
pub fn find_next(conn: &Connection, source: &str, o: &SearchOptions) -> Result<Option<SearchHit>> {
    let columns: Vec<String> = describe(conn, source)?.into_iter().map(|c| c.name).collect();
    let Some(sql) = build_search_sql(source, &columns, o) else { return Ok(None) };
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    match rows.next()? {
        Some(r) => Ok(Some(SearchHit {
            row: r.get::<_, i64>(0)? as u64,
            column_index: r.get::<_, i64>(1)? as u64,
            column: r.get(2)?,
            value: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
        })),
        None => Ok(None),
    }
}
