//! Window operations: cumulative values, shift/diff/percent change, rank and
//! rolling statistics, optionally per group (pandas `groupby().cumsum()`
//! and friends). The result keeps the table's row order.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::ops::{OpContext, ROW_NUMBER, StepLabel, with_row_number};
use crate::sql::quote_ident as q;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowFunc {
    CumSum,
    CumMin,
    CumMax,
    CumMean,
    /// 0, 1, 2… within each group (pandas `cumcount`).
    CumCount,
    /// Value `periods` rows earlier (negative: later).
    Shift,
    /// Value minus the value `periods` rows earlier.
    Diff,
    /// Relative change from the value `periods` rows earlier.
    PctChange,
    Rank,
    /// Rank as a fraction between 0 and 1.
    PercentRank,
    /// 1, 2, 3… within each group.
    RowNumber,
    RollingMean,
    RollingSum,
    RollingMin,
    RollingMax,
    RollingStd,
}

impl WindowFunc {
    /// The serde name, also used in step labels (the UI translates `window.<name>`).
    pub fn name(self) -> &'static str {
        match self {
            WindowFunc::CumSum => "cum_sum",
            WindowFunc::CumMin => "cum_min",
            WindowFunc::CumMax => "cum_max",
            WindowFunc::CumMean => "cum_mean",
            WindowFunc::CumCount => "cum_count",
            WindowFunc::Shift => "shift",
            WindowFunc::Diff => "diff",
            WindowFunc::PctChange => "pct_change",
            WindowFunc::Rank => "rank",
            WindowFunc::PercentRank => "percent_rank",
            WindowFunc::RowNumber => "row_number",
            WindowFunc::RollingMean => "rolling_mean",
            WindowFunc::RollingSum => "rolling_sum",
            WindowFunc::RollingMin => "rolling_min",
            WindowFunc::RollingMax => "rolling_max",
            WindowFunc::RollingStd => "rolling_std",
        }
    }

    fn needs_column(self) -> bool {
        !matches!(self, WindowFunc::CumCount | WindowFunc::RowNumber)
    }
}

/// How ties are ranked (pandas `rank(method=…)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RankMethod {
    #[default]
    Average,
    Min,
    Max,
    Dense,
    First,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowSpec {
    /// The input column; unused for `cum_count` and `row_number`.
    #[serde(default)]
    pub column: String,
    pub func: WindowFunc,
    /// Rows for shift/diff/pct_change (default 1).
    #[serde(default)]
    pub periods: Option<i64>,
    /// Rows in the rolling window (default 3).
    #[serde(default)]
    pub window: Option<u32>,
    #[serde(default)]
    pub rank_method: RankMethod,
    /// Rank or order direction.
    #[serde(default = "default_true")]
    pub ascending: bool,
    /// Compute separately within each group of these columns.
    #[serde(default)]
    pub partition_by: Vec<String>,
    /// Order rows by this column; default is the table's row order.
    #[serde(default)]
    pub order_by: Option<String>,
    pub output: String,
}

pub(crate) fn window(ctx: &OpContext, s: &WindowSpec) -> Result<(String, StepLabel)> {
    if s.func.needs_column() {
        ctx.require(&s.column)?;
    }
    for p in &s.partition_by {
        ctx.require(p)?;
    }
    let order_col = s.order_by.as_deref().map(str::trim).filter(|o| !o.is_empty());
    if let Some(o) = order_col {
        ctx.require(o)?;
    }
    let out = s.output.trim();
    if out.is_empty() {
        return Err(CoreError::invalid("Output column name is empty"));
    }
    let rn = q(ROW_NUMBER);
    let c = q(&s.column);
    let partition = if s.partition_by.is_empty() {
        String::new()
    } else {
        format!("PARTITION BY {} ", s.partition_by.iter().map(|p| q(p)).collect::<Vec<_>>().join(", "))
    };
    // Row order for the running functions: the chosen column, then table order.
    let dir = if s.ascending { "ASC" } else { "DESC" };
    let order = match order_col {
        Some(o) => format!("ORDER BY {} {dir} NULLS LAST, {rn}", q(o)),
        None => format!("ORDER BY {rn}"),
    };
    let running = format!("OVER ({partition}{order} ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW)");
    let ordered = format!("OVER ({partition}{order})");
    let periods = s.periods.unwrap_or(1);
    let lag = |n: i64| {
        if n >= 0 { format!("LAG({c}, {n}) {ordered}") } else { format!("LEAD({c}, {}) {ordered}", -n) }
    };
    let window_rows = s.window.unwrap_or(3);
    let rolling = |agg: &str| -> Result<String> {
        if window_rows == 0 {
            return Err(CoreError::invalid("The rolling window must be at least 1 row"));
        }
        let frame = format!("OVER ({partition}{order} ROWS BETWEEN {} PRECEDING AND CURRENT ROW)", window_rows - 1);
        // Like pandas: no value until the window holds `window` non-null values.
        Ok(format!("CASE WHEN COUNT({c}) {frame} >= {window_rows} THEN {agg}({c}) {frame} END"))
    };
    let rank_order = format!("ORDER BY {c} {dir}");
    let rank_over = format!("OVER ({partition}{rank_order})");
    let ties = {
        let mut keys: Vec<String> = s.partition_by.iter().map(|p| q(p)).collect();
        keys.push(c.clone());
        format!("COUNT(*) OVER (PARTITION BY {})", keys.join(", "))
    };
    let expr = match s.func {
        WindowFunc::CumSum => format!("SUM({c}) {running}"),
        WindowFunc::CumMin => format!("MIN({c}) {running}"),
        WindowFunc::CumMax => format!("MAX({c}) {running}"),
        WindowFunc::CumMean => format!("AVG({c}) {running}"),
        WindowFunc::CumCount => format!("ROW_NUMBER() {ordered} - 1"),
        WindowFunc::RowNumber => format!("ROW_NUMBER() {ordered}"),
        WindowFunc::Shift => lag(periods),
        WindowFunc::Diff => format!("{c} - {}", lag(periods)),
        WindowFunc::PctChange => format!("{c} / NULLIF({}, 0) - 1", lag(periods)),
        WindowFunc::PercentRank => format!("CASE WHEN {c} IS NOT NULL THEN PERCENT_RANK() {rank_over} END"),
        WindowFunc::Rank => {
            let r = match s.rank_method {
                RankMethod::Min => format!("RANK() {rank_over}"),
                RankMethod::Dense => format!("DENSE_RANK() {rank_over}"),
                RankMethod::First => format!("ROW_NUMBER() OVER ({partition}{rank_order}, {rn})"),
                RankMethod::Max => format!("RANK() {rank_over} + {ties} - 1"),
                RankMethod::Average => format!("RANK() {rank_over} + ({ties} - 1) / 2.0"),
            };
            // NULLs get no rank, as in pandas.
            format!("CASE WHEN {c} IS NOT NULL THEN {r} END")
        }
        WindowFunc::RollingMean => rolling("AVG")?,
        WindowFunc::RollingSum => rolling("SUM")?,
        WindowFunc::RollingMin => rolling("MIN")?,
        WindowFunc::RollingMax => rolling("MAX")?,
        WindowFunc::RollingStd => rolling("STDDEV_SAMP")?,
    };
    let src = with_row_number(ctx.current_query);
    let sql = if ctx.columns.iter().any(|m| m.name == out) {
        format!("SELECT * EXCLUDE ({rn}) REPLACE ({expr} AS {}) FROM ({src}) ORDER BY {rn}", q(out))
    } else {
        format!("SELECT * EXCLUDE ({rn}), {expr} AS {} FROM ({src}) ORDER BY {rn}", q(out))
    };
    let target = if s.func.needs_column() { s.column.as_str() } else { "—" };
    Ok((sql, StepLabel::new("step.window", &[("func", s.func.name()), ("column", target), ("output", out)])))
}
