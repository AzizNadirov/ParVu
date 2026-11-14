//! Data for charts. DuckDB aggregates or samples the tab's current result, so
//! a chart of a multi-gigabyte file sends at most a few thousand points to
//! the UI, which only draws them.

use duckdb::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::ops::with_row_number;
use crate::page::ColumnMeta;
use crate::reshape::AggFunc;
use crate::sql::quote_ident as q;

/// Most bars in a bar chart.
pub const MAX_BARS: usize = 50;
/// Most points in a line chart.
pub const MAX_LINE_POINTS: usize = 5000;
/// Points sampled for a scatter plot.
pub const SCATTER_SAMPLE: usize = 5000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartKind {
    /// One bar per value of `x`, height = aggregation of `y`.
    Bar,
    /// Aggregation of `y` for each value of `x`, in `x` order.
    Line,
    /// One point per row (a sample), `x` and `y` numeric.
    Scatter,
    /// Number of rows per interval of numeric `x`.
    Histogram,
}

fn default_agg() -> AggFunc {
    AggFunc::Sum
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartSpec {
    pub kind: ChartKind,
    pub x: String,
    /// Value column; `None` counts rows (bar and line).
    #[serde(default)]
    pub y: Option<String>,
    #[serde(default = "default_agg")]
    pub agg: AggFunc,
    /// Intervals for a histogram (default 20).
    #[serde(default)]
    pub bins: Option<u32>,
    /// Bar charts: largest first instead of by label.
    #[serde(default)]
    pub sort_by_value: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChartData {
    /// Category or x label of each point (bar, line, histogram).
    pub labels: Vec<String>,
    /// Bar heights, line values or histogram counts; for scatter, y values.
    pub values: Vec<Option<f64>>,
    /// Scatter only: x values.
    pub xs: Vec<Option<f64>>,
    /// True when the chart shows only part of the data (too many bars or points).
    pub truncated: bool,
}

fn rows(conn: &Connection, sql: &str) -> Result<Vec<(Option<String>, Option<f64>)>> {
    let mut stmt = conn.prepare(sql)?;
    let out = stmt.query_map([], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<f64>>(1)?)))?;
    Ok(out.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// Build the chart data for `spec` over `source` (the tab's current query).
pub fn chart_data(conn: &Connection, source: &str, columns: &[ColumnMeta], spec: &ChartSpec) -> Result<ChartData> {
    let has = |c: &str| columns.iter().any(|m| m.name == c);
    if !has(&spec.x) {
        return Err(CoreError::UnknownColumn(spec.x.clone()));
    }
    let y = spec.y.as_deref().map(str::trim).filter(|y| !y.is_empty());
    if let Some(y) = y
        && !has(y)
    {
        return Err(CoreError::UnknownColumn(y.to_string()));
    }
    let x = q(&spec.x);
    match spec.kind {
        ChartKind::Bar | ChartKind::Line => {
            if spec.agg == AggFunc::List {
                return Err(CoreError::invalid("A list cannot be charted; choose another calculation"));
            }
            let (agg, value) = match y {
                Some(y) => (spec.agg, q(y)),
                None => (AggFunc::Rows, String::new()),
            };
            let value_sql = format!("TRY_CAST({} AS DOUBLE)", agg.sql(&value, None));
            let src = if agg.needs_order() { with_row_number(source) } else { source.to_string() };
            let (order, limit) = match (spec.kind, spec.sort_by_value) {
                (ChartKind::Bar, true) => ("v DESC NULLS LAST, k".to_string(), MAX_BARS),
                (ChartKind::Bar, false) => ("k NULLS LAST".to_string(), MAX_BARS),
                _ => ("k NULLS LAST".to_string(), MAX_LINE_POINTS),
            };
            // Group by the raw value so lines follow numeric and date order.
            let sql = format!(
                "SELECT CAST(k AS VARCHAR), v FROM (SELECT {x} AS k, {value_sql} AS v FROM ({src}) GROUP BY {x}) \
                 ORDER BY {order} LIMIT {}",
                limit + 1
            );
            let mut r = rows(conn, &sql)?;
            let truncated = r.len() > limit;
            r.truncate(limit);
            Ok(ChartData {
                labels: r.iter().map(|(k, _)| k.clone().unwrap_or_else(|| "NULL".into())).collect(),
                values: r.into_iter().map(|(_, v)| v).collect(),
                xs: Vec::new(),
                truncated,
            })
        }
        ChartKind::Scatter => {
            let Some(y) = y else {
                return Err(CoreError::invalid("Choose a column for the vertical axis"));
            };
            let total: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM ({source})"), [], |r| r.get(0))?;
            let sql = format!(
                "SELECT TRY_CAST({x} AS DOUBLE), TRY_CAST({} AS DOUBLE) FROM ({}) USING SAMPLE reservoir({SCATTER_SAMPLE} ROWS) REPEATABLE (42)",
                q(y),
                source
            );
            let mut stmt = conn.prepare(&sql)?;
            let pts = stmt
                .query_map([], |r| Ok((r.get::<_, Option<f64>>(0)?, r.get::<_, Option<f64>>(1)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(ChartData {
                labels: Vec::new(),
                xs: pts.iter().map(|p| p.0).collect(),
                values: pts.iter().map(|p| p.1).collect(),
                truncated: total as usize > SCATTER_SAMPLE,
            })
        }
        ChartKind::Histogram => {
            let bins = spec.bins.unwrap_or(20).clamp(1, 200);
            let v = format!("TRY_CAST({x} AS DOUBLE)");
            let (lo, hi): (Option<f64>, Option<f64>) =
                conn.query_row(&format!("SELECT MIN({v}), MAX({v}) FROM ({source})"), [], |r| Ok((r.get(0)?, r.get(1)?)))?;
            let (Some(lo), Some(hi)) = (lo, hi) else {
                return Err(CoreError::invalid(format!("\"{}\" has no numeric values", spec.x)));
            };
            let width = if hi > lo { (hi - lo) / f64::from(bins) } else { 1.0 };
            let sql = format!(
                "SELECT CAST(LEAST(FLOOR(({v} - {lo:?}) / {width:?}), {}) AS VARCHAR), CAST(COUNT(*) AS DOUBLE) \
                 FROM ({source}) WHERE {v} IS NOT NULL GROUP BY 1",
                bins - 1
            );
            let mut counts = vec![0.0; bins as usize];
            for (b, n) in rows(conn, &sql)? {
                if let Some(i) = b.and_then(|b| b.parse::<f64>().ok()) {
                    counts[(i as usize).min(bins as usize - 1)] = n.unwrap_or(0.0);
                }
            }
            let fmt = |f: f64| {
                let s = format!("{:.4}", f);
                let s = s.trim_end_matches('0').trim_end_matches('.');
                if s == "-0" { "0".to_string() } else { s.to_string() }
            };
            Ok(ChartData {
                labels: (0..bins).map(|i| fmt(lo + width * f64::from(i))).collect(),
                values: counts.into_iter().map(Some).collect(),
                xs: vec![lo, lo + width * f64::from(bins)].into_iter().map(Some).collect(),
                truncated: false,
            })
        }
    }
}
