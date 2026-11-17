//! Running a definition's examples on the template (plans/CUSTOM_TOOLS_PLAN.md
//! §4.5). Examples run on the compiled template, not the source, so a
//! wrong round trip through DuckDB's serializer fails an example.

use std::collections::BTreeMap;

use duckdb::Connection;
use serde::{Deserialize, Serialize};

use super::compile::{ARGS_TABLE, arg_slot};
use super::fill::{fill_slots, value_literal, with_tables};
use super::{DefKind, Definition, Example, ExampleArg, ExampleColumn, ExampleTable, ParamKind, ResultKind};
use crate::engine::{describe, query_batches};
use crate::error::{CoreError, Result};
use crate::page::{Cell, format_batches};
use crate::sql::{is_safe_type_name, quote_ident, quote_literal};

/// Row number column added to function inputs, to read results in order.
const ROW: &str = "__pv_rn";

/// What one example did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExampleOutcome {
    pub name: String,
    pub passed: bool,
    /// Why it could not run (a DuckDB error, an argument that doesn't fit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The output it gave.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<ExampleTable>,
    /// What differs: `[row, column]` of an expected cell; `[row, -1]` an
    /// expected row with no equal row in the output; `[-1, -1]` the column
    /// names; `[-2, -1]` output rows that were not expected.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mismatches: Vec<[i64; 2]>,
}

/// `SELECT … FROM (VALUES …)` for an example table; an empty table is a
/// typed `WHERE false` select. With `row`, a row number column is added.
pub fn table_sql(t: &ExampleTable, row: bool) -> Result<String> {
    if t.columns.is_empty() {
        return Err(CoreError::invalid("An example table has no columns"));
    }
    for c in &t.columns {
        if !is_safe_type_name(&c.dtype) {
            return Err(CoreError::invalid(format!("Column {}: invalid type \"{}\"", c.name, c.dtype)));
        }
    }
    let mut names: Vec<String> = t.columns.iter().map(|c| quote_ident(&c.name)).collect();
    if row {
        names.insert(0, quote_ident(ROW));
    }
    let cast = |v: &Cell, dtype: &str| match v {
        None => format!("CAST(NULL AS {dtype})"),
        Some(s) => format!("CAST({} AS {dtype})", quote_literal(s)),
    };
    if t.rows.is_empty() {
        let items: Vec<String> = t.columns.iter().map(|c| format!("{} AS {}", cast(&None, &c.dtype), quote_ident(&c.name))).collect();
        let rn = if row { format!("CAST(NULL AS BIGINT) AS {}, ", quote_ident(ROW)) } else { String::new() };
        return Ok(format!("SELECT {rn}{} WHERE false", items.join(", ")));
    }
    let rows: Vec<String> = t
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            if r.len() != t.columns.len() {
                return Err(CoreError::invalid(format!("Row {} has {} cells, expected {}", i + 1, r.len(), t.columns.len())));
            }
            let mut cells: Vec<String> = r.iter().zip(&t.columns).map(|(v, c)| cast(v, &c.dtype)).collect();
            if row {
                cells.insert(0, i.to_string());
            }
            Ok(format!("({})", cells.join(", ")))
        })
        .collect::<Result<_>>()?;
    Ok(format!("SELECT * FROM (VALUES {}) AS v({})", rows.join(", "), names.join(", ")))
}

/// The SQL that runs `def`'s template on example `ex`.
pub fn example_sql(def: &Definition, template: &str, ex: &Example) -> Result<String> {
    let arg = |name: &str| ex.args.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone());
    let mut slots = vec![String::new(); def.params.len()];
    for (i, p) in def.params.iter().enumerate() {
        match p.kind {
            ParamKind::Table => {}
            ParamKind::Value => {
                let raw: Option<String> = match arg(&p.name) {
                    Some(ExampleArg::Cell(v)) => v,
                    Some(ExampleArg::Columns(_)) => return Err(CoreError::invalid(format!("{}: expected a value", p.name))),
                    None if p.optional => p.default.clone(),
                    None => return Err(CoreError::invalid(format!("No value for {}", p.name))),
                };
                slots[i] = format!("({})", value_literal(p, raw.as_deref())?);
            }
            ParamKind::Column | ParamKind::Columns => {
                let columns: Vec<String> = match (p.kind, def.kind, arg(&p.name)) {
                    (ParamKind::Columns, _, Some(ExampleArg::Columns(list))) => list,
                    (ParamKind::Columns, _, Some(ExampleArg::Cell(Some(c)))) => vec![c],
                    (ParamKind::Columns, _, _) => return Err(CoreError::invalid(format!("Choose the columns for {}", p.name))),
                    (_, DefKind::Function, _) => vec![p.name.clone()],
                    (_, DefKind::Operation, a) => vec![a.and_then(|a| a.cell().map(str::to_string)).ok_or_else(|| {
                        CoreError::invalid(format!("Choose a column for {}", p.name))
                    })?],
                };
                if p.kind == ParamKind::Columns {
                    super::fill::check_count(p, &columns)?;
                }
                let table = match def.kind {
                    DefKind::Function => Some(&ex.input),
                    DefKind::Operation => {
                        let t = def.table_of(p).map(|t| t.name.clone()).unwrap_or_default();
                        if def.tables().next().is_some_and(|f| f.name == t) { Some(&ex.input) } else { ex.tables.get(&t) }
                    }
                };
                for column in &columns {
                    if !table.is_some_and(|t| t.columns.iter().any(|c| c.name.eq_ignore_ascii_case(column))) {
                        return Err(CoreError::invalid(format!("The example has no column {column} for {}", p.name)));
                    }
                }
                slots[i] = columns.iter().map(|c| quote_ident(c)).collect::<Vec<_>>().join(", ");
            }
        }
    }
    let filled = fill_slots(template, &|i| slots.get(i).cloned().unwrap_or_else(|| arg_slot(i)));
    match def.kind {
        DefKind::Function => {
            let input = table_sql(&ex.input, true)?;
            let args = quote_ident(ARGS_TABLE);
            Ok(match def.result.kind {
                ResultKind::Value => format!("SELECT {filled} AS result FROM ({input}) AS {args}"),
                _ => format!("SELECT {filled} AS result FROM ({input}) AS {args} ORDER BY {}", quote_ident(ROW)),
            })
        }
        DefKind::Operation => {
            let mut tables = vec![table_sql(&ex.input, false)?];
            for t in def.tables().skip(1) {
                let table = ex.tables.get(&t.name).ok_or_else(|| CoreError::invalid(format!("The example has no table {}", t.name)))?;
                tables.push(table_sql(table, false)?);
            }
            Ok(with_tables(&tables, &filled))
        }
    }
}

/// Run `sql` and return its output as an example table.
pub fn run_sql(conn: &Connection, sql: &str) -> Result<ExampleTable> {
    let columns = describe(conn, sql)?;
    let rows = format_batches(&query_batches(conn, sql)?)?;
    Ok(ExampleTable {
        columns: columns.into_iter().map(|c| ExampleColumn { name: c.name, dtype: c.dtype }).collect(),
        rows,
    })
}

/// Cells equal as text, or as numbers to 9 significant digits.
pub fn same_cell(expected: &Cell, actual: &Cell) -> bool {
    match (expected, actual) {
        (None, None) => true,
        (Some(e), Some(a)) => {
            if e == a {
                return true;
            }
            match (e.trim().parse::<f64>(), a.trim().parse::<f64>()) {
                (Ok(x), Ok(y)) if x.is_finite() && y.is_finite() => format!("{x:.8e}") == format!("{y:.8e}"),
                _ => e.trim().eq_ignore_ascii_case(a.trim()) && is_bool(e),
            }
        }
        _ => false,
    }
}

fn is_bool(s: &str) -> bool {
    matches!(s.trim().to_lowercase().as_str(), "true" | "false")
}

/// Compare an output with the expected table; returns the mismatches.
pub fn compare(def: &Definition, ex: &Example, actual: &ExampleTable) -> Vec<[i64; 2]> {
    let mut out = Vec::new();
    let expected = &ex.expected;
    // Functions give one unnamed column; operations compare names too.
    if def.kind == DefKind::Operation {
        let names_a: Vec<&str> = actual.columns.iter().map(|c| c.name.as_str()).collect();
        let names_e: Vec<&str> = expected.columns.iter().map(|c| c.name.as_str()).collect();
        if names_a != names_e {
            out.push([-1, -1]);
            return out;
        }
    }
    let width = expected.columns.len().max(1);
    if def.kind == DefKind::Function && actual.columns.len() != 1 {
        out.push([-1, -1]);
        return out;
    }
    let ordered = ex.ordered || def.kind == DefKind::Function;
    if ordered {
        let n = expected.rows.len().max(actual.rows.len());
        for r in 0..n {
            match (expected.rows.get(r), actual.rows.get(r)) {
                (Some(e), Some(a)) => {
                    for c in 0..width {
                        if !same_cell(&e.get(c).cloned().flatten(), &a.get(c).cloned().flatten()) {
                            out.push([r as i64, c as i64]);
                        }
                    }
                }
                (Some(_), None) => out.push([r as i64, -1]),
                _ => out.push([-2, -1]),
            }
        }
    } else {
        // As a multiset: each expected row takes one equal actual row.
        let mut free: Vec<bool> = vec![true; actual.rows.len()];
        for (r, e) in expected.rows.iter().enumerate() {
            let hit = actual.rows.iter().enumerate().position(|(i, a)| {
                free[i] && e.len() == a.len() && e.iter().zip(a).all(|(x, y)| same_cell(x, y))
            });
            match hit {
                Some(i) => free[i] = false,
                None => out.push([r as i64, -1]),
            }
        }
        if free.iter().any(|f| *f) {
            out.push([-2, -1]);
        }
    }
    out.dedup();
    out
}

/// Run one example: build the SQL, run it, compare.
pub fn run_example(conn: &Connection, def: &Definition, template: &str, ex: &Example) -> ExampleOutcome {
    let mut outcome = ExampleOutcome { name: ex.name.clone(), ..Default::default() };
    let result = example_sql(def, template, ex).and_then(|sql| run_sql(conn, &sql));
    match result {
        Ok(actual) => {
            outcome.mismatches = compare(def, ex, &actual);
            outcome.passed = outcome.mismatches.is_empty();
            outcome.actual = Some(actual);
        }
        Err(e) => outcome.error = Some(clean_error(&e.to_string())),
    }
    outcome
}

/// DuckDB's message without the "LINE 1: …" excerpt of internal SQL.
pub fn clean_error(msg: &str) -> String {
    msg.split("\n\nLINE ").next().unwrap_or(msg).replace("__pv_args.", "").trim().to_string()
}

/// Map of argument name to value, for building examples in tests.
pub fn args<const N: usize>(pairs: [(&str, Option<&str>); N]) -> BTreeMap<String, ExampleArg> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), ExampleArg::Cell(v.map(str::to_string)))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(v: &[Option<&str>]) -> Vec<Cell> {
        v.iter().map(|c| c.map(str::to_string)).collect()
    }

    #[test]
    fn numbers_compare_to_nine_digits() {
        assert!(same_cell(&Some("1.666666667".into()), &Some("1.6666666666666667".into())));
        assert!(same_cell(&Some("22.5".into()), &Some("22.50".into())));
        assert!(!same_cell(&Some("1.67".into()), &Some("1.6666666666666667".into())));
        assert!(same_cell(&Some("TRUE".into()), &Some("true".into())));
        assert!(!same_cell(&Some("Abc".into()), &Some("abc".into())));
        assert!(!same_cell(&None, &Some("".into())));
    }

    #[test]
    fn tables_become_values() {
        let t = ExampleTable {
            columns: vec![ExampleColumn { name: "a".into(), dtype: "BIGINT".into() }],
            rows: vec![cells(&[Some("1")]), cells(&[None])],
        };
        let conn = Connection::open_in_memory().unwrap();
        let out = run_sql(&conn, &table_sql(&t, false).unwrap()).unwrap();
        assert_eq!(out.rows, vec![cells(&[Some("1")]), cells(&[None])]);
        let empty = ExampleTable { rows: vec![], ..t.clone() };
        assert!(run_sql(&conn, &table_sql(&empty, true).unwrap()).unwrap().rows.is_empty());
        let bad = ExampleTable { columns: vec![ExampleColumn { name: "a".into(), dtype: "INT); DROP".into() }], rows: vec![] };
        assert!(table_sql(&bad, false).is_err());
    }
}
