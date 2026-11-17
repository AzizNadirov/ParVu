//! Column formulas: columns computed by an expression of each row, kept
//! live as part of the table (between the cell edits and the steps).
//!
//! The expression arrives compiled (the app compiles the expression
//! language; this crate stays free of it). Its column references are
//! qualified with the table's name, so every layer aliases its input with
//! that name.

use duckdb::Connection;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::page::ColumnMeta;
use crate::sql::{is_safe_type_name, quote_ident, quote_literal};

/// A computed column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Formula {
    /// The column it makes.
    pub name: String,
    /// What the user typed, e.g. `[qty] * [price]`.
    pub source: String,
    /// The compiled SQL expression.
    pub sql: String,
    /// Shown right after this column; at the end when `None` or unknown.
    #[serde(default)]
    pub after: Option<String>,
    /// Column totals the formula uses (`SUM([price])`), as aggregate SQL.
    /// `sql` holds [`total_marker`]`(i)` where total `i` goes: each total is
    /// computed once when the views are built (after every cell edit too),
    /// so pages don't rescan the table.
    #[serde(default)]
    pub totals: Vec<String>,
}

/// Where total `i` goes in a formula's SQL. The expression language's
/// compiler writes the same text (see `parvu_dsl::compile_formula`).
pub fn total_marker(i: usize) -> String {
    format!("\u{1}pv_total_{i}\u{1}")
}

/// `formulas` with their totals computed over the rows they see (`base`
/// with the cell edits, plus the formulas before them) and written into
/// their SQL as constants.
pub fn with_totals(conn: &Connection, base: &str, table: &str, formulas: &[Formula]) -> Result<Vec<Formula>> {
    let mut done: Vec<Formula> = Vec::with_capacity(formulas.len());
    for f in formulas {
        if f.totals.is_empty() {
            done.push(f.clone());
            continue;
        }
        let input = layers(base, table, &done);
        let parts: Vec<String> = f
            .totals
            .iter()
            .enumerate()
            .map(|(i, t)| format!("CAST(({t}) AS VARCHAR) AS v{i}, typeof(({t})) AS t{i}"))
            .collect();
        let values: Vec<(Option<String>, String)> = crate::sql::query_whole_table(conn, &parts.join(", "), &input, &quote_ident(table), |r| {
            (0..f.totals.len()).map(|i| Ok((r.get(2 * i)?, r.get(2 * i + 1)?))).collect()
        })?;
        let mut resolved = f.sql.clone();
        for (i, (value, ty)) in values.iter().enumerate() {
            let constant = match value {
                None => "NULL".to_string(),
                Some(v) if is_safe_type_name(ty) => format!("CAST({} AS {ty})", quote_literal(v)),
                Some(v) => quote_literal(v),
            };
            resolved = resolved.replace(&total_marker(i), &constant);
        }
        done.push(Formula { sql: resolved, totals: Vec::new(), ..f.clone() });
    }
    Ok(done)
}

/// `base` with each formula as one more column, in order.
/// The row number that keeps the file's order when a formula uses a window
/// (`COUNTIF([region], [region])`, `PREV([x])`): DuckDB returns a window's
/// rows grouped by partition, which reordered the table (and the saved file).
const ROW: &str = "__pv_frn";

fn windowed(formulas: &[Formula]) -> bool {
    formulas.iter().any(|f| f.sql.contains(" OVER ("))
}

fn layers(base: &str, table: &str, formulas: &[Formula]) -> String {
    let ordered = windowed(formulas);
    let mut query = if ordered {
        format!("SELECT *, ROW_NUMBER() OVER () AS {} FROM {}", quote_ident(ROW), quote_ident(base))
    } else {
        format!("SELECT * FROM {}", quote_ident(base))
    };
    for f in formulas {
        // Neighbouring rows (PREV / NEXT) follow the row number, not the
        // order an earlier window left the rows in.
        let sql = if ordered { f.sql.replace(" OVER ()", &format!(" OVER (ORDER BY {})", quote_ident(ROW))) } else { f.sql.clone() };
        query = format!("SELECT *, ({sql}) AS {} FROM ({query}) AS {}", quote_ident(&f.name), quote_ident(table));
    }
    query
}

/// The table's view: `base` (the file with its cell edits) plus each
/// formula, in order. A formula sees the columns of `base` and of the
/// formulas before it, so formulas can't refer to each other in a cycle.
pub fn formula_sql(base: &str, table: &str, base_columns: &[ColumnMeta], formulas: &[Formula]) -> String {
    let query = layers(base, table, formulas);
    if formulas.is_empty() {
        return query;
    }
    let mut order: Vec<&str> = base_columns.iter().map(|c| c.name.as_str()).collect();
    for f in formulas {
        let at = f.after.as_deref().and_then(|a| order.iter().position(|c| *c == a)).map(|i| i + 1).unwrap_or(order.len());
        order.insert(at, &f.name);
    }
    let list: Vec<String> = order.iter().map(|c| quote_ident(c)).collect();
    if windowed(formulas) {
        format!("SELECT {} FROM ({query}) ORDER BY {}", list.join(", "), quote_ident(ROW))
    } else {
        format!("SELECT {} FROM ({query})", list.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_and_order() {
        let cols = vec![ColumnMeta::new("qty", "BIGINT"), ColumnMeta::new("price", "DOUBLE")];
        assert_eq!(formula_sql("b", "t", &cols, &[]), "SELECT * FROM \"b\"");
        let f = |name: &str, sql: &str, after: Option<&str>| Formula {
            name: name.into(),
            source: String::new(),
            sql: sql.into(),
            after: after.map(str::to_string),
            totals: Vec::new(),
        };
        let sql = formula_sql("b", "t", &cols, &[f("total", "t.qty * t.price", None), f("half", "t.total / 2", Some("qty"))]);
        assert_eq!(
            sql,
            "SELECT \"qty\", \"half\", \"price\", \"total\" FROM (SELECT *, (t.total / 2) AS \"half\" FROM \
             (SELECT *, (t.qty * t.price) AS \"total\" FROM (SELECT * FROM \"b\") AS \"t\") AS \"t\")"
        );
    }
}
