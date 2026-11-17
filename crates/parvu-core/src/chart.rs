//! Data for charts. DuckDB aggregates or samples the tab's current result, so
//! a chart of a multi-gigabyte file sends at most a few thousand points to
//! the UI, which only draws them.
//!
//! * Categories: the largest `limit` are shown, the rest folded into one
//!   "Other" category, aggregated over its rows (not summed from the shown
//!   values, so means and medians stay right).
//! * Dates and timestamps: grouped by day, week, month, quarter or year;
//!   `auto` picks the unit from the date range.
//! * `series`: a second column splits the values into up to
//!   [`MAX_SERIES`] named series plus "Other" (scatter: [`MAX_SCATTER_SERIES`],
//!   the most colour-distinct hues).

use duckdb::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::ops::with_row_number;
use crate::page::ColumnMeta;
use crate::reshape::AggFunc;
use crate::sql::quote_ident as q;

/// Most categories a chart may show.
pub const MAX_CATEGORIES: u32 = 100;
/// Categories shown when the spec doesn't say.
pub const DEFAULT_CATEGORIES: u32 = 20;
/// Most points along a time or number axis.
pub const MAX_LINE_POINTS: usize = 5000;
/// Points sampled for a scatter plot.
pub const SCATTER_SAMPLE: usize = 5000;
/// Named series before the rest are folded into "Other".
pub const MAX_SERIES: usize = 7;
/// Named series in a scatter plot (every pair of points must stay apart).
pub const MAX_SCATTER_SERIES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartKind {
    /// One bar per value of `x`, length = aggregation of `y`.
    Bar,
    /// Aggregation of `y` along `x`.
    Line,
    /// A line with a filled area (stacked when split into series).
    Area,
    /// One point per row (a sample), `x` and `y` numeric.
    Scatter,
    /// Number of rows per interval of numeric `x`.
    Histogram,
}

/// How dates on `x` are grouped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeUnit {
    /// Picked from the date range (about 60 groups at most).
    #[default]
    Auto,
    /// Every distinct value.
    None,
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

impl TimeUnit {
    fn sql(self) -> Option<&'static str> {
        match self {
            TimeUnit::Day => Some("day"),
            TimeUnit::Week => Some("week"),
            TimeUnit::Month => Some("month"),
            TimeUnit::Quarter => Some("quarter"),
            TimeUnit::Year => Some("year"),
            TimeUnit::Auto | TimeUnit::None => None,
        }
    }

    /// The coarsest-needed unit giving at most ~60 groups over `days`.
    pub fn for_span(days: i64) -> TimeUnit {
        match days {
            d if d <= 60 => TimeUnit::Day,
            d if d <= 60 * 7 => TimeUnit::Week,
            d if d <= 60 * 31 => TimeUnit::Month,
            d if d <= 60 * 92 => TimeUnit::Quarter,
            _ => TimeUnit::Year,
        }
    }

    /// Label of a group from its ISO date ("2024-04-01").
    fn label(self, iso: &str) -> String {
        let d = iso.get(..10).unwrap_or(iso);
        match self {
            TimeUnit::Month => d.get(..7).unwrap_or(d).to_string(),
            TimeUnit::Year => d.get(..4).unwrap_or(d).to_string(),
            TimeUnit::Quarter => {
                let m: u32 = d.get(5..7).and_then(|m| m.parse().ok()).unwrap_or(1);
                format!("{} Q{}", d.get(..4).unwrap_or(d), (m - 1) / 3 + 1)
            }
            TimeUnit::Day | TimeUnit::Week => d.to_string(),
            TimeUnit::Auto | TimeUnit::None => iso.to_string(),
        }
    }
}

/// Order of categories along the axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    /// Largest first for categories; natural order for dates and numbers.
    #[default]
    Auto,
    ValueDesc,
    ValueAsc,
    Label,
}

/// What kind of values run along `x`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum XKind {
    Category,
    Time,
    Number,
}

fn default_agg() -> AggFunc {
    AggFunc::Sum
}
fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartSpec {
    pub kind: ChartKind,
    pub x: String,
    /// Value column; `None` counts rows (bar, line, area).
    #[serde(default)]
    pub y: Option<String>,
    #[serde(default = "default_agg")]
    pub agg: AggFunc,
    /// Column whose values split the data into series.
    #[serde(default)]
    pub series: Option<String>,
    /// Grouping of dates on `x`.
    #[serde(default)]
    pub time_unit: TimeUnit,
    #[serde(default)]
    pub sort: SortOrder,
    /// Categories shown (default 20, at most 100).
    #[serde(default)]
    pub limit: Option<u32>,
    /// Fold the categories past `limit` into "Other" instead of dropping them.
    #[serde(default = "yes")]
    pub other: bool,
    /// Intervals for a histogram (default 20).
    #[serde(default)]
    pub bins: Option<u32>,
}

/// One series of values, one per label (scatter: one per point, with `xs`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Series {
    /// Value of the split column; empty when the chart isn't split.
    pub name: String,
    /// True for the series that collects the values past the limit.
    pub other: bool,
    pub values: Vec<Option<f64>>,
    /// Scatter only: x of each point.
    pub xs: Vec<Option<f64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChartData {
    pub x_kind: XKind,
    /// The date grouping used (`auto` resolved), for time axes.
    pub time_unit: Option<TimeUnit>,
    /// One label per position along x (not scatter).
    pub labels: Vec<String>,
    /// Index of the label that is the folded "Other" category, if any.
    pub other_label: Option<usize>,
    pub series: Vec<Series>,
    /// Histogram: the interval edges (labels.len() + 1).
    pub bin_edges: Vec<f64>,
    /// True when the chart shows only part of the data (sampled or cut).
    pub truncated: bool,
}

fn is_time(dtype: &str) -> bool {
    let d = dtype.to_uppercase();
    d.starts_with("DATE") || d.starts_with("TIMESTAMP")
}

fn kind_of(col: &ColumnMeta) -> XKind {
    if is_time(&col.dtype) {
        XKind::Time
    } else if col.is_numeric() {
        XKind::Number
    } else {
        XKind::Category
    }
}

/// Build the chart data for `spec` over `source` (the tab's current query).
pub fn chart_data(conn: &Connection, source: &str, columns: &[ColumnMeta], spec: &ChartSpec) -> Result<ChartData> {
    let col = |c: &str| columns.iter().find(|m| m.name == c).ok_or_else(|| CoreError::UnknownColumn(c.to_string()));
    let x_col = col(&spec.x)?;
    let y = spec.y.as_deref().map(str::trim).filter(|y| !y.is_empty());
    if let Some(y) = y {
        col(y)?;
    }
    let split = spec.series.as_deref().map(str::trim).filter(|s| !s.is_empty() && *s != spec.x);
    if let Some(s) = split {
        col(s)?;
    }
    match spec.kind {
        ChartKind::Bar | ChartKind::Line | ChartKind::Area => grouped(conn, source, x_col, y, split, spec),
        ChartKind::Scatter => scatter(conn, source, &spec.x, y, split),
        ChartKind::Histogram => histogram(conn, source, &spec.x, spec.bins),
    }
}

/// Bar, line and area: an aggregate per x position (and series).
fn grouped(
    conn: &Connection,
    source: &str,
    x_col: &ColumnMeta,
    y: Option<&str>,
    split: Option<&str>,
    spec: &ChartSpec,
) -> Result<ChartData> {
    if spec.agg == AggFunc::List {
        return Err(CoreError::invalid("A list cannot be charted; choose another calculation"));
    }
    let (agg, value) = match y {
        Some(y) => (spec.agg, q(y)),
        None => (AggFunc::Rows, String::new()),
    };
    let agg_sql = format!("TRY_CAST({} AS DOUBLE)", agg.sql(&value, None));
    let src = if agg.needs_order() { with_row_number(source) } else { source.to_string() };
    let x = q(&x_col.name);
    let mut x_kind = kind_of(x_col);

    // Dates: group by a unit.
    let mut unit = None;
    let key = if x_kind == XKind::Time {
        let u = match spec.time_unit {
            TimeUnit::Auto => {
                let days: Option<i64> = conn.query_row(
                    &format!("SELECT date_diff('day', CAST(MIN({x}) AS DATE), CAST(MAX({x}) AS DATE)) FROM ({src})"),
                    [],
                    |r| r.get(0),
                )?;
                TimeUnit::for_span(days.unwrap_or(0))
            }
            u => u,
        };
        unit = Some(u);
        match u.sql() {
            Some(s) => format!("CAST(date_trunc('{s}', {x}) AS DATE)"),
            None => x.clone(),
        }
    } else {
        x.clone()
    };

    // Categories are cut to the largest `limit`; dates and numbers keep
    // their order and are only capped.
    let categorical = x_kind == XKind::Category;
    let limit = if categorical {
        spec.limit.unwrap_or(DEFAULT_CATEGORIES).clamp(1, MAX_CATEGORIES) as usize
    } else {
        MAX_LINE_POINTS
    };
    let fold = categorical && spec.other;

    let base = match split {
        Some(s) => format!("SELECT *, {key} AS __pv_k, {} AS __pv_s FROM ({src})", q(s)),
        None => format!("SELECT *, {key} AS __pv_k, NULL AS __pv_s FROM ({src})"),
    };
    let top_k = format!(
        "SELECT __pv_k FROM base GROUP BY __pv_k ORDER BY {agg_sql} DESC NULLS LAST, __pv_k LIMIT {limit}"
    );
    let top_s = format!(
        "SELECT __pv_s FROM base GROUP BY __pv_s ORDER BY {agg_sql} DESC NULLS LAST, __pv_s LIMIT {MAX_SERIES}"
    );
    let in_k = "EXISTS (SELECT 1 FROM top_k t WHERE t.__pv_k IS NOT DISTINCT FROM base.__pv_k)";
    let in_s = "EXISTS (SELECT 1 FROM top_s t WHERE t.__pv_s IS NOT DISTINCT FROM base.__pv_s)";
    // Only categories are cut by value; a time or number axis keeps every
    // position in order (up to the cap).
    let where_k = if categorical && !fold { format!("WHERE {in_k}") } else { String::new() };
    let k_other = if fold { format!("NOT {in_k}") } else { "false".into() };
    let sql = format!(
        "WITH base AS ({base}), top_k AS ({top_k}), top_s AS ({top_s}) \
         SELECT CAST(k AS VARCHAR), TRY_CAST(k AS DOUBLE), ko, CAST(s AS VARCHAR), so, v FROM ( \
           SELECT CASE WHEN ko THEN NULL ELSE __pv_k END AS k, ko, \
                  CASE WHEN so THEN NULL ELSE __pv_s END AS s, so, {agg_sql} AS v \
           FROM (SELECT *, {k_other} AS ko, NOT {in_s} AS so FROM base {where_k}) \
           GROUP BY ALL) \
         ORDER BY ko, k NULLS LAST LIMIT {}",
        (limit + 1) * (MAX_SERIES + 1) + 1
    );
    type Row = (Option<String>, Option<f64>, bool, Option<String>, bool, Option<f64>);
    let mut stmt = conn.prepare(&sql)?;
    let rows: Vec<Row> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
        .collect::<std::result::Result<_, _>>()?;

    // Positions along x: (key, numeric key, is_other).
    let mut keys: Vec<(Option<String>, Option<f64>, bool)> = Vec::new();
    for (k, kn, ko, ..) in &rows {
        if !keys.iter().any(|(a, _, o)| a == k && o == ko) {
            keys.push((k.clone(), *kn, *ko));
        }
    }
    let truncated = !categorical && keys.len() > limit;
    keys.truncate(limit + usize::from(fold));
    // Series: named ones by total, "Other" last.
    let mut names: Vec<(Option<String>, bool, f64)> = Vec::new();
    for (_, _, _, s, so, v) in &rows {
        match names.iter_mut().find(|(n, o, _)| n == s && o == so) {
            Some(e) => e.2 += v.unwrap_or(0.0),
            None => names.push((s.clone(), *so, v.unwrap_or(0.0))),
        }
    }
    names.sort_by(|a, b| a.1.cmp(&b.1).then(b.2.total_cmp(&a.2)).then(a.0.cmp(&b.0)));

    let value = |k: &(Option<String>, Option<f64>, bool), s: &(Option<String>, bool, f64)| {
        rows.iter()
            .find(|(rk, _, rko, rs, rso, _)| rk == &k.0 && *rko == k.2 && rs == &s.0 && *rso == s.1)
            .and_then(|r| r.5)
    };
    let total = |k: &(Option<String>, Option<f64>, bool)| -> f64 { names.iter().filter_map(|s| value(k, s)).sum() };

    // Order along x.
    let sort = match spec.sort {
        SortOrder::Auto if categorical => SortOrder::ValueDesc,
        SortOrder::Auto => SortOrder::Label,
        s => s,
    };
    let numeric_keys = keys.iter().all(|(k, kn, o)| *o || k.is_none() || kn.is_some());
    keys.sort_by(|a, b| {
        // "Other" always last, then empty values.
        a.2.cmp(&b.2).then_with(|| match sort {
            SortOrder::ValueDesc => total(b).total_cmp(&total(a)),
            SortOrder::ValueAsc => total(a).total_cmp(&total(b)),
            _ if numeric_keys && x_kind != XKind::Time => {
                a.1.unwrap_or(f64::INFINITY).total_cmp(&b.1.unwrap_or(f64::INFINITY))
            }
            _ => a.0.is_none().cmp(&b.0.is_none()).then_with(|| a.0.cmp(&b.0)),
        })
    });

    if x_kind == XKind::Number && sort != SortOrder::Label {
        // Sorted by value, numbers are just names.
        x_kind = XKind::Category;
    }
    let labels = keys
        .iter()
        .map(|(k, _, o)| {
            if *o {
                "Other".to_string()
            } else {
                match (k, unit) {
                    (None, _) => "(empty)".to_string(),
                    (Some(k), Some(u)) => u.label(k),
                    (Some(k), None) => k.clone(),
                }
            }
        })
        .collect();
    let series = names
        .iter()
        .map(|s| Series {
            name: if split.is_none() {
                String::new()
            } else if s.1 {
                "Other".into()
            } else {
                s.0.clone().unwrap_or_else(|| "(empty)".into())
            },
            other: s.1,
            values: keys.iter().map(|k| value(k, s)).collect(),
            xs: Vec::new(),
        })
        .collect::<Vec<_>>();
    let series = if series.is_empty() {
        vec![Series { name: String::new(), other: false, values: Vec::new(), xs: Vec::new() }]
    } else {
        series
    };
    Ok(ChartData {
        x_kind,
        time_unit: unit,
        other_label: keys.iter().position(|k| k.2),
        labels,
        series,
        bin_edges: Vec::new(),
        truncated,
    })
}

fn scatter(conn: &Connection, source: &str, x: &str, y: Option<&str>, split: Option<&str>) -> Result<ChartData> {
    let Some(y) = y else {
        return Err(CoreError::invalid("Choose a column for the vertical axis"));
    };
    let total: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM ({source})"), [], |r| r.get(0))?;
    let s_expr = split.map(q).unwrap_or_else(|| "NULL".into());
    // The named series: the most frequent values of the split column.
    let tops: Vec<Option<String>> = match split {
        Some(s) => {
            let sql = format!(
                "SELECT CAST({} AS VARCHAR) FROM ({source}) GROUP BY 1 ORDER BY COUNT(*) DESC, 1 LIMIT {}",
                q(s),
                MAX_SCATTER_SERIES + 2
            );
            let mut stmt = conn.prepare(&sql)?;
            let mut tops: Vec<Option<String>> =
                stmt.query_map([], |r| r.get::<_, Option<String>>(0))?.collect::<std::result::Result<_, _>>()?;
            // "Other" only when it holds at least two values: with one more
            // value than the cap, that value is shown by name.
            if tops.len() > MAX_SCATTER_SERIES + 1 {
                tops.truncate(MAX_SCATTER_SERIES);
            }
            tops
        }
        None => vec![None],
    };
    let sql = format!(
        "SELECT TRY_CAST({} AS DOUBLE), TRY_CAST({} AS DOUBLE), CAST({s_expr} AS VARCHAR) \
         FROM ({source}) USING SAMPLE reservoir({SCATTER_SAMPLE} ROWS) REPEATABLE (42)",
        q(x),
        q(y)
    );
    let mut stmt = conn.prepare(&sql)?;
    let pts: Vec<(Option<f64>, Option<f64>, Option<String>)> =
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<std::result::Result<_, _>>()?;
    let mut series: Vec<Series> = tops
        .iter()
        .map(|t| Series {
            name: if split.is_some() { t.clone().unwrap_or_else(|| "(empty)".into()) } else { String::new() },
            other: false,
            values: Vec::new(),
            xs: Vec::new(),
        })
        .collect();
    let mut other = Series { name: "Other".into(), other: true, values: Vec::new(), xs: Vec::new() };
    for (px, py, ps) in pts {
        let target = match tops.iter().position(|t| split.is_none() || *t == ps) {
            Some(i) => &mut series[i],
            None => &mut other,
        };
        target.xs.push(px);
        target.values.push(py);
    }
    if !other.values.is_empty() {
        series.push(other);
    }
    Ok(ChartData {
        x_kind: XKind::Number,
        time_unit: None,
        labels: Vec::new(),
        other_label: None,
        series,
        bin_edges: Vec::new(),
        truncated: total as usize > SCATTER_SAMPLE,
    })
}

fn histogram(conn: &Connection, source: &str, x: &str, bins: Option<u32>) -> Result<ChartData> {
    let bins = bins.unwrap_or(20).clamp(1, 200);
    let v = format!("TRY_CAST({} AS DOUBLE)", q(x));
    let (lo, hi): (Option<f64>, Option<f64>) =
        conn.query_row(&format!("SELECT MIN({v}), MAX({v}) FROM ({source})"), [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let (Some(lo), Some(hi)) = (lo, hi) else {
        return Err(CoreError::invalid(format!("\"{x}\" has no numeric values")));
    };
    // Round edges (a width of 1, 2, 2.5 or 5 × 10ⁿ starting on a multiple of
    // it) read far better than min + k·(max − min)/n; the count of
    // intervals may then differ a little from the one asked for.
    let width = nice_bin_width(hi - lo, bins);
    let lo = (lo / width).floor() * width;
    let bins = (((hi - lo) / width).ceil() as u32).max(1);
    let sql = format!(
        "SELECT CAST(LEAST(FLOOR(({v} - {lo:?}) / {width:?}), {}) AS VARCHAR), CAST(COUNT(*) AS DOUBLE) \
         FROM ({source}) WHERE {v} IS NOT NULL GROUP BY 1",
        bins - 1
    );
    let mut counts = vec![0.0; bins as usize];
    let mut stmt = conn.prepare(&sql)?;
    let rows: Vec<(Option<String>, Option<f64>)> =
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<std::result::Result<_, _>>()?;
    for (b, n) in rows {
        if let Some(i) = b.and_then(|b| b.parse::<f64>().ok()) {
            counts[(i as usize).min(bins as usize - 1)] = n.unwrap_or(0.0);
        }
    }
    let edges: Vec<f64> = (0..=bins).map(|i| round_sig(lo + width * f64::from(i))).collect();
    let fmt = |f: f64| {
        let s = format!("{:.4}", f);
        let s = s.trim_end_matches('0').trim_end_matches('.');
        if s == "-0" { "0".to_string() } else { s.to_string() }
    };
    Ok(ChartData {
        x_kind: XKind::Number,
        time_unit: None,
        labels: (0..bins as usize).map(|i| format!("{} – {}", fmt(edges[i]), fmt(edges[i + 1]))).collect(),
        other_label: None,
        series: vec![Series { name: String::new(), other: false, values: counts.into_iter().map(Some).collect(), xs: Vec::new() }],
        bin_edges: edges,
        truncated: false,
    })
}

/// Interval width for about `bins` intervals over `span`: 1, 2, 2.5 or 5 × 10ⁿ.
fn nice_bin_width(span: f64, bins: u32) -> f64 {
    if span.is_nan() || span <= 0.0 || !span.is_finite() {
        return 1.0;
    }
    let raw = span / f64::from(bins.max(1));
    let mag = 10f64.powf(raw.log10().floor());
    [1.0, 2.0, 2.5, 5.0, 10.0].iter().map(|m| m * mag).find(|w| *w >= raw * (1.0 - 1e-9)).unwrap_or(10.0 * mag)
}

/// `v` without float noise (0.30000000000000004 → 0.3).
fn round_sig(v: f64) -> f64 {
    if v == 0.0 || !v.is_finite() {
        return v;
    }
    format!("{v:.12e}").parse().unwrap_or(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_units() {
        assert_eq!(TimeUnit::for_span(10), TimeUnit::Day);
        assert_eq!(TimeUnit::for_span(200), TimeUnit::Week);
        assert_eq!(TimeUnit::for_span(1000), TimeUnit::Month);
        assert_eq!(TimeUnit::for_span(4000), TimeUnit::Quarter);
        assert_eq!(TimeUnit::for_span(20000), TimeUnit::Year);
        assert_eq!(TimeUnit::Quarter.label("2024-08-01"), "2024 Q3");
        assert_eq!(TimeUnit::Month.label("2024-08-01 00:00:00"), "2024-08");
        assert_eq!(TimeUnit::Year.label("2024-01-01"), "2024");
    }

    #[test]
    fn bin_widths_are_round() {
        assert_eq!(nice_bin_width(1242.5, 20), 100.0);
        assert_eq!(nice_bin_width(100.0, 20), 5.0);
        assert_eq!(nice_bin_width(1.0, 10), 0.1);
        assert_eq!(nice_bin_width(45.0, 20), 2.5);
        assert_eq!(nice_bin_width(0.0, 20), 1.0);
        assert_eq!(round_sig(0.1 + 0.2), 0.3);
    }
}
