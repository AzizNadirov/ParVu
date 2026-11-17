//! Column statistics, distinct values and samples.

use duckdb::Connection;
use duckdb::types::Value;
use serde::{Deserialize, Serialize};

use std::collections::HashSet;

use duckdb::arrow::util::display::ArrayFormatter;

use crate::error::{CoreError, Result};
use crate::job::Cancel;
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
    /// `distinct` is an estimate (HyperLogLog, within a few percent): exact
    /// counting on a big table can need more memory than there is.
    #[serde(default)]
    pub distinct_approx: bool,
    pub min: Option<String>,
    pub max: Option<String>,
    pub mean: Option<f64>,
    pub std: Option<f64>,
}

/// Tables with more rows than this get an estimated distinct count.
pub const EXACT_DISTINCT_ROWS: u64 = 2_000_000;

/// Summary statistics of `column` over `source`. With `approx`, the
/// distinct count is estimated in constant memory.
pub fn column_stats(conn: &Connection, source: &str, column: &str, dtype: &str, approx: bool) -> Result<ColumnStats> {
    let q = quote_ident(column);
    let distinct = if approx { format!("approx_count_distinct({q})") } else { format!("COUNT(DISTINCT {q})") };
    // Mean and standard deviation only for numbers: on a text column they
    // came from the few values that happened to look like numbers.
    let (mean, std) = if crate::page::ColumnMeta::new("", dtype).is_numeric() {
        (format!("AVG(TRY_CAST({q} AS DOUBLE))"), format!("STDDEV(TRY_CAST({q} AS DOUBLE))"))
    } else {
        ("CAST(NULL AS DOUBLE)".to_string(), "CAST(NULL AS DOUBLE)".to_string())
    };
    let items = format!(
        "COUNT(*), COUNT({q}), COUNT(*) - COUNT({q}), {distinct}, \
         CAST(MIN({q}) AS VARCHAR), CAST(MAX({q}) AS VARCHAR), {mean}, {std}"
    );
    let stats = crate::sql::query_whole_table(conn, &items, source, "__pv_t", |r| {
        Ok(ColumnStats {
            column: column.to_string(),
            dtype: dtype.to_string(),
            row_count: r.get::<_, i64>(0)? as u64,
            non_null: r.get::<_, i64>(1)? as u64,
            null: r.get::<_, i64>(2)? as u64,
            distinct: r.get::<_, i64>(3)? as u64,
            distinct_approx: approx,
            min: r.get(4)?,
            max: r.get(5)?,
            mean: r.get::<_, Option<f64>>(6)?.filter(|v| v.is_finite()),
            std: r.get::<_, Option<f64>>(7)?.filter(|v| v.is_finite()),
        })
    })?;
    Ok(stats)
}

/// Text of the distinct values kept at most (8 MiB): a column of long texts
/// stops before the list gets too big to send and show.
pub const UNIQUE_MAX_BYTES: usize = 8 << 20;

/// Why a unique-values scan stopped before the end of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UniqueStop {
    /// More than [`UNIQUE_LIMIT`] different values.
    TooMany,
    /// The values' text passed [`UNIQUE_MAX_BYTES`].
    TooLarge,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniqueValues {
    /// Sorted: NULL first, numbers by value, then text.
    pub values: Vec<Cell>,
    /// Every row was read.
    pub complete: bool,
    pub stopped: Option<UniqueStop>,
    pub rows_scanned: u64,
}

/// How often `on_batch` is called at most while scanning.
const UNIQUE_REPORT: std::time::Duration = std::time::Duration::from_millis(200);

/// Distinct values of `column` as text (NULL as `None`), read piece by piece.
///
/// The column is streamed in batches and de-duplicated here, so memory stays
/// small whatever the table's size, and the scan stops as soon as there are
/// more than [`UNIQUE_LIMIT`] values (or [`UNIQUE_MAX_BYTES`] of text): a
/// column of ids answers after its first rows. `on_batch(rows_scanned,
/// new_values)` reports progress at most every 200 ms. `cancel` is checked
/// between batches; the running statement is stopped by interrupting `conn`.
pub fn unique_values(
    conn: &Connection,
    source: &str,
    column: &str,
    cancel: &Cancel,
    on_batch: &mut dyn FnMut(u64, &[Cell]),
) -> Result<UniqueValues> {
    cancel.check()?;
    let q = quote_ident(column);
    let mut stmt = conn.prepare(&format!("SELECT CAST({q} AS VARCHAR) FROM ({source})"))?;
    let stream = stmt.stream_arrow([])?;
    let opts = crate::page::format_options();
    let mut seen: HashSet<Cell> = HashSet::new();
    let mut fresh: Vec<Cell> = Vec::new();
    let mut bytes = 0usize;
    let mut rows = 0u64;
    let mut stopped = None;
    let mut last_report = std::time::Instant::now();
    'scan: for batch in stream {
        cancel.check()?;
        let batch = batch?;
        let array = batch.column(0);
        let f = ArrayFormatter::try_new(array.as_ref(), &opts).map_err(|e| CoreError::invalid(e.to_string()))?;
        for r in 0..batch.num_rows() {
            let cell = if array.is_null(r) { None } else { Some(f.value(r).to_string()) };
            if seen.contains(&cell) {
                continue;
            }
            if seen.len() >= UNIQUE_LIMIT {
                stopped = Some(UniqueStop::TooMany);
                rows += r as u64;
                break 'scan;
            }
            bytes += cell.as_ref().map_or(0, String::len);
            if bytes > UNIQUE_MAX_BYTES {
                stopped = Some(UniqueStop::TooLarge);
                rows += r as u64;
                break 'scan;
            }
            seen.insert(cell.clone());
            fresh.push(cell);
        }
        rows += batch.num_rows() as u64;
        if last_report.elapsed() >= UNIQUE_REPORT {
            on_batch(rows, &fresh);
            fresh.clear();
            last_report = std::time::Instant::now();
        }
    }
    // Interrupted between batches: a cancel, not a complete list.
    cancel.check()?;
    let mut values: Vec<Cell> = seen.into_iter().collect();
    values.sort_by(|a, b| compare_values(a.as_deref(), b.as_deref()));
    Ok(UniqueValues { values, complete: stopped.is_none(), stopped, rows_scanned: rows })
}

/// NULL first, then numbers by value, then text.
pub fn compare_values(a: Option<&str>, b: Option<&str>) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    match (a, b) {
        (None, None) => Equal,
        (None, _) => Less,
        (_, None) => Greater,
        (Some(a), Some(b)) => match (a.trim().parse::<f64>().ok(), b.trim().parse::<f64>().ok()) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Equal).then_with(|| a.cmp(b)),
            (Some(_), None) => Less,
            (None, Some(_)) => Greater,
            (None, None) => a.cmp(b),
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SampleMode {
    First,
    Random,
}

/// Most values copied as a tuple.
pub const TUPLE_LIMIT: u64 = 100_000;

/// Minimum and maximum of each column read as numbers (colour scales), in
/// one scan; `None` for a column without numbers.
pub fn numeric_ranges(conn: &Connection, source: &str, columns: &[String]) -> Result<Vec<Option<(f64, f64)>>> {
    if columns.is_empty() {
        return Ok(Vec::new());
    }
    let parts: Vec<String> = columns
        .iter()
        .map(|c| {
            let q = quote_ident(c);
            format!("MIN(TRY_CAST({q} AS DOUBLE)), MAX(TRY_CAST({q} AS DOUBLE))")
        })
        .collect();
    let sql = format!("SELECT {} FROM ({source})", parts.join(", "));
    Ok(conn.query_row(&sql, [], |r| {
        (0..columns.len())
            .map(|i| Ok(r.get::<_, Option<f64>>(2 * i)?.zip(r.get::<_, Option<f64>>(2 * i + 1)?)))
            .collect::<std::result::Result<Vec<_>, _>>()
    })?)
}

/// A sample of `column` values, typed, for "copy as tuple" (at most
/// [`TUPLE_LIMIT`]).
pub fn column_sample(conn: &Connection, source: &str, column: &str, mode: SampleMode, n: u64) -> Result<Vec<Value>> {
    let q = quote_ident(column);
    let n = n.min(TUPLE_LIMIT);
    // A reservoir sample reads the column once in constant memory; ORDER BY
    // RANDOM() would sort the whole table.
    let sql = match mode {
        SampleMode::First => format!("SELECT {q} FROM ({source}) LIMIT {n}"),
        SampleMode::Random => format!("SELECT {q} FROM (SELECT {q} FROM ({source})) USING SAMPLE reservoir({n} ROWS)"),
    };
    let mut stmt = conn.prepare(&sql)?;
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

/// Rows read for value suggestions: the first ones only, so suggesting is
/// quick on any file.
pub const SUGGESTION_ROWS: u64 = 200_000;

/// Values to suggest for `column` (a filter's value box): its most frequent
/// values among the first [`SUGGESTION_ROWS`] rows of `source`, as text.
pub fn value_suggestions(conn: &Connection, source: &str, column: &str, limit: u64) -> Result<Vec<String>> {
    let c = quote_ident(column);
    let sql = format!(
        "SELECT CAST({c} AS VARCHAR) AS v, COUNT(*) AS n FROM (SELECT {c} FROM ({source}) LIMIT {SUGGESTION_ROWS}) \
         WHERE {c} IS NOT NULL GROUP BY 1 ORDER BY n DESC, v LIMIT {limit}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
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
