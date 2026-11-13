//! Column statistics, distinct values and samples.

use duckdb::Connection;
use duckdb::types::Value;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::page::Cell;
use crate::sql::quote_ident;

/// Maximum distinct values returned for the filter dialog.
pub const UNIQUE_LIMIT: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnStats {
    pub column: String,
    pub dtype: String,
    pub row_count: u64,
    pub non_null: u64,
    pub null: u64,
    pub distinct: u64,
    pub min: Option<String>,
    pub max: Option<String>,
    pub mean: Option<f64>,
    pub std: Option<f64>,
}

/// Summary statistics of `column` over `source`.
pub fn column_stats(conn: &Connection, source: &str, column: &str, dtype: &str) -> Result<ColumnStats> {
    let q = quote_ident(column);
    let sql = format!(
        "SELECT COUNT(*), COUNT({q}), COUNT(*) - COUNT({q}), COUNT(DISTINCT {q}), \
         CAST(MIN({q}) AS VARCHAR), CAST(MAX({q}) AS VARCHAR), \
         AVG(TRY_CAST({q} AS DOUBLE)), STDDEV(TRY_CAST({q} AS DOUBLE)) FROM ({source})"
    );
    let stats = conn.query_row(&sql, [], |r| {
        Ok(ColumnStats {
            column: column.to_string(),
            dtype: dtype.to_string(),
            row_count: r.get::<_, i64>(0)? as u64,
            non_null: r.get::<_, i64>(1)? as u64,
            null: r.get::<_, i64>(2)? as u64,
            distinct: r.get::<_, i64>(3)? as u64,
            min: r.get(4)?,
            max: r.get(5)?,
            mean: r.get::<_, Option<f64>>(6)?.filter(|v| v.is_finite()),
            std: r.get::<_, Option<f64>>(7)?.filter(|v| v.is_finite()),
        })
    })?;
    Ok(stats)
}

fn text_values(conn: &Connection, sql: &str) -> Result<Vec<Cell>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| r.get::<_, Option<String>>(0))?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// Distinct values of `column` as text (NULL as `None`), sorted, at most [`UNIQUE_LIMIT`].
pub fn unique_values(conn: &Connection, source: &str, column: &str) -> Result<Vec<Cell>> {
    let q = quote_ident(column);
    text_values(
        conn,
        &format!(
            "SELECT CAST(v AS VARCHAR) FROM (SELECT DISTINCT {q} AS v FROM ({source}) LIMIT {UNIQUE_LIMIT}) \
             ORDER BY v NULLS FIRST"
        ),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SampleMode {
    First,
    Random,
}

/// A sample of `column` values, typed, for "copy as tuple".
pub fn column_sample(conn: &Connection, source: &str, column: &str, mode: SampleMode, n: u64) -> Result<Vec<Value>> {
    let q = quote_ident(column);
    let order = match mode {
        SampleMode::First => "",
        SampleMode::Random => " ORDER BY RANDOM()",
    };
    let mut stmt = conn.prepare(&format!("SELECT {q} FROM ({source}){order} LIMIT {n}"))?;
    let rows = stmt.query_map([], |r| r.get::<_, Value>(0))?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// Render values like Python's `repr` inside a tuple: `(1, 'a', None)`.
pub fn python_tuple(values: &[Value]) -> String {
    let parts: Vec<String> = values.iter().map(python_repr).collect();
    format!("({})", parts.join(", "))
}

fn python_repr(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::Boolean(b) => if *b { "True".into() } else { "False".into() },
        Value::TinyInt(i) => i.to_string(),
        Value::SmallInt(i) => i.to_string(),
        Value::Int(i) => i.to_string(),
        Value::BigInt(i) => i.to_string(),
        Value::HugeInt(i) => i.to_string(),
        Value::UTinyInt(i) => i.to_string(),
        Value::USmallInt(i) => i.to_string(),
        Value::UInt(i) => i.to_string(),
        Value::UBigInt(i) => i.to_string(),
        Value::Float(f) => float_repr(*f as f64),
        Value::Double(f) => float_repr(*f),
        Value::Text(s) => py_str(s),
        other => py_str(&format!("{other:?}")),
    }
}

fn float_repr(f: f64) -> String {
    if f.is_nan() {
        "nan".into()
    } else if f.is_infinite() {
        if f > 0.0 { "inf".into() } else { "-inf".into() }
    } else if f.fract() == 0.0 && f.abs() < 1e16 {
        format!("{f:.1}")
    } else {
        f.to_string()
    }
}

fn py_str(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') { '"' } else { '\'' };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// Parse a "copy as tuple" sample size.
pub fn parse_sample_size(n: i64) -> Result<u64> {
    if n <= 0 { Err(CoreError::invalid("Sample size must be positive")) } else { Ok(n as u64) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tuple_formatting() {
        let v = vec![Value::BigInt(1), Value::Text("it's".into()), Value::Null, Value::Double(2.0), Value::Boolean(true)];
        assert_eq!(python_tuple(&v), "(1, \"it's\", None, 2.0, True)");
        assert_eq!(python_tuple(&[Value::BigInt(5)]), "(5)");
        assert_eq!(python_tuple(&[]), "()");
        assert_eq!(py_str("a'b\"c"), "'a\\'b\"c'");
    }
}
