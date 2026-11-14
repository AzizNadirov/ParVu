//! Reshaping operations: group by, pivot, melt, transpose, value counts and
//! describe. Each function returns the SQL wrapping the tab's current query
//! and the step label; [`crate::ops::Operation::build`] dispatches to them.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::ops::{OpContext, ROW_NUMBER, StepLabel, unique_name, with_row_number};
use crate::page::Cell;
use crate::sql::{quote_ident as q, quote_literal};

/// Most distinct values a pivot turns into columns.
pub const PIVOT_MAX_COLUMNS: usize = 200;
/// Most rows a table can have to be transposed.
pub const TRANSPOSE_MAX_ROWS: u64 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggFunc {
    /// Non-null values (pandas `count`).
    Count,
    /// All rows, NULLs included (pandas `size`).
    Rows,
    /// Distinct non-null values (pandas `nunique`).
    CountDistinct,
    Sum,
    Mean,
    Median,
    Min,
    Max,
    Std,
    Var,
    /// First non-null value in table order.
    First,
    /// Last non-null value in table order.
    Last,
    /// Most frequent value.
    Mode,
    /// All values as a list, in table order.
    List,
}

impl AggFunc {
    pub fn name(self) -> &'static str {
        match self {
            AggFunc::Count => "count",
            AggFunc::Rows => "rows",
            AggFunc::CountDistinct => "count_distinct",
            AggFunc::Sum => "sum",
            AggFunc::Mean => "mean",
            AggFunc::Median => "median",
            AggFunc::Min => "min",
            AggFunc::Max => "max",
            AggFunc::Std => "std",
            AggFunc::Var => "var",
            AggFunc::First => "first",
            AggFunc::Last => "last",
            AggFunc::Mode => "mode",
            AggFunc::List => "list",
        }
    }

    /// Whether the SQL needs the row-number column (functions that follow table order).
    fn needs_order(self) -> bool {
        matches!(self, AggFunc::First | AggFunc::Last | AggFunc::List)
    }

    /// Aggregate SQL over `col` (already quoted; ignored for `Rows`), with an
    /// optional extra FILTER condition.
    fn sql(self, col: &str, filter: Option<&str>) -> String {
        let rn = q(ROW_NUMBER);
        let (expr, own_filter) = match self {
            AggFunc::Count => (format!("COUNT({col})"), None),
            AggFunc::Rows => ("COUNT(*)".to_string(), None),
            AggFunc::CountDistinct => (format!("COUNT(DISTINCT {col})"), None),
            AggFunc::Sum => (format!("SUM({col})"), None),
            AggFunc::Mean => (format!("AVG({col})"), None),
            AggFunc::Median => (format!("MEDIAN({col})"), None),
            AggFunc::Min => (format!("MIN({col})"), None),
            AggFunc::Max => (format!("MAX({col})"), None),
            AggFunc::Std => (format!("STDDEV_SAMP({col})"), None),
            AggFunc::Var => (format!("VAR_SAMP({col})"), None),
            AggFunc::First => (format!("ARG_MIN({col}, {rn})"), Some(format!("{col} IS NOT NULL"))),
            AggFunc::Last => (format!("ARG_MAX({col}, {rn})"), Some(format!("{col} IS NOT NULL"))),
            AggFunc::Mode => (format!("MODE({col})"), None),
            AggFunc::List => (format!("LIST({col} ORDER BY {rn})"), None),
        };
        let conds: Vec<String> = own_filter.into_iter().chain(filter.map(str::to_string)).collect();
        if conds.is_empty() { expr } else { format!("{expr} FILTER (WHERE {})", conds.join(" AND ")) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Aggregation {
    /// Column to aggregate; may be empty for [`AggFunc::Rows`].
    #[serde(default)]
    pub column: String,
    pub func: AggFunc,
    /// Output column name; defaults to `<column>_<func>` (or `rows`).
    #[serde(default)]
    pub alias: Option<String>,
}

fn source_for(ctx: &OpContext, ordered: bool) -> String {
    if ordered { with_row_number(ctx.current_query) } else { ctx.current_query.to_string() }
}

fn key_list(by: &[String]) -> String {
    by.iter().map(|c| q(c)).collect::<Vec<_>>().join(", ")
}

pub(crate) fn group_by(ctx: &OpContext, by: &[String], aggregations: &[Aggregation]) -> Result<(String, StepLabel)> {
    for c in by {
        ctx.require(c)?;
    }
    if aggregations.is_empty() {
        return Err(CoreError::invalid("Add at least one aggregation"));
    }
    let mut used: Vec<String> = by.to_vec();
    let mut items: Vec<String> = by.iter().map(|c| q(c)).collect();
    for a in aggregations {
        if a.func != AggFunc::Rows {
            ctx.require(&a.column)?;
        }
        let default = if a.func == AggFunc::Rows { "rows".to_string() } else { format!("{}_{}", a.column, a.func.name()) };
        let wanted = a.alias.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or(&default);
        let name = unique_name(&used, wanted);
        used.push(name.clone());
        items.push(format!("{} AS {}", a.func.sql(&q(&a.column), None), q(&name)));
    }
    let ordered = aggregations.iter().any(|a| a.func.needs_order());
    let src = source_for(ctx, ordered);
    let mut sql = format!("SELECT {} FROM ({src})", items.join(", "));
    if !by.is_empty() {
        let keys = key_list(by);
        sql.push_str(&format!(" GROUP BY {keys} ORDER BY {keys} NULLS LAST"));
    }
    let funcs: Vec<String> = aggregations
        .iter()
        .map(|a| if a.func == AggFunc::Rows { "rows".to_string() } else { format!("{}({})", a.func.name(), a.column) })
        .collect();
    let by_text = if by.is_empty() { "—".to_string() } else { by.join(", ") };
    Ok((sql, StepLabel::new("step.group_by", &[("columns", &by_text), ("aggregations", &funcs.join(", "))])))
}

pub(crate) fn pivot(
    ctx: &OpContext,
    index: &[String],
    columns: &str,
    values: Option<&str>,
    func: AggFunc,
) -> Result<(String, StepLabel)> {
    for c in index {
        ctx.require(c)?;
    }
    ctx.require(columns)?;
    let values = values.map(str::trim).filter(|v| !v.is_empty());
    match values {
        Some(v) => ctx.require(v)?,
        None if func != AggFunc::Rows && func != AggFunc::Count => {
            return Err(CoreError::invalid("Choose a values column for this aggregation"));
        }
        None => {}
    }
    if index.iter().any(|c| c == columns) {
        return Err(CoreError::invalid("The pivot column cannot also be a row column"));
    }
    let pc = q(columns);
    let distinct = (ctx.query)(&format!(
        "SELECT DISTINCT CAST({pc} AS VARCHAR) AS v FROM ({}) ORDER BY v NULLS LAST LIMIT {}",
        ctx.current_query,
        PIVOT_MAX_COLUMNS + 1
    ))?;
    if distinct.len() > PIVOT_MAX_COLUMNS {
        return Err(CoreError::invalid(format!(
            "\"{columns}\" has more than {PIVOT_MAX_COLUMNS} distinct values; pivot a column with fewer values"
        )));
    }
    let value_col = values.map(q).unwrap_or_default();
    let func = if values.is_none() { AggFunc::Rows } else { func };
    let mut used: Vec<String> = index.to_vec();
    let mut items: Vec<String> = index.iter().map(|c| q(c)).collect();
    for row in &distinct {
        let v: &Cell = row.first().unwrap_or(&None);
        let cond = match v {
            Some(text) => format!("CAST({pc} AS VARCHAR) = {}", quote_literal(text)),
            None => format!("{pc} IS NULL"),
        };
        let name = unique_name(&used, v.as_deref().unwrap_or("NULL"));
        used.push(name.clone());
        items.push(format!("{} AS {}", func.sql(&value_col, Some(&cond)), q(&name)));
    }
    let src = source_for(ctx, func.needs_order());
    let mut sql = format!("SELECT {} FROM ({src})", items.join(", "));
    if !index.is_empty() {
        let keys = key_list(index);
        sql.push_str(&format!(" GROUP BY {keys} ORDER BY {keys} NULLS LAST"));
    }
    let value_text = values.map(|v| format!("{}({v})", func.name())).unwrap_or_else(|| "rows".to_string());
    Ok((
        sql,
        StepLabel::new(
            "step.pivot",
            &[("index", &if index.is_empty() { "—".to_string() } else { index.join(", ") }), ("columns", columns), ("values", &value_text)],
        ),
    ))
}

pub(crate) fn melt(
    ctx: &OpContext,
    id_columns: &[String],
    value_columns: &[String],
    variable_name: &str,
    value_name: &str,
) -> Result<(String, StepLabel)> {
    for c in id_columns.iter().chain(value_columns) {
        ctx.require(c)?;
    }
    let values: Vec<&str> = if value_columns.is_empty() {
        ctx.columns.iter().map(|c| c.name.as_str()).filter(|n| !id_columns.iter().any(|i| i == n)).collect()
    } else {
        value_columns.iter().map(String::as_str).collect()
    };
    if values.is_empty() {
        return Err(CoreError::invalid("No columns left to unpivot"));
    }
    let (var, val) = (variable_name.trim(), value_name.trim());
    if var.is_empty() || val.is_empty() || var == val {
        return Err(CoreError::invalid("Give the name and value columns two different names"));
    }
    if id_columns.iter().any(|c| c == var || c == val) {
        return Err(CoreError::invalid("The name or value column clashes with a kept column"));
    }
    // UNPIVOT needs one type for the value column; mixed types become text.
    let types: Vec<&str> = values
        .iter()
        .filter_map(|v| ctx.columns.iter().find(|c| c.name == *v).map(|c| c.dtype.as_str()))
        .collect();
    let same_type = types.windows(2).all(|w| w[0] == w[1]);
    let mut select: Vec<String> = id_columns.iter().map(|c| q(c)).collect();
    select.extend(values.iter().map(|v| if same_type { q(v) } else { format!("CAST({} AS VARCHAR) AS {}", q(v), q(v)) }));
    let on = values.iter().map(|v| q(v)).collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT * FROM (SELECT {} FROM ({})) UNPIVOT INCLUDE NULLS ({} FOR {} IN ({on}))",
        select.join(", "),
        ctx.current_query,
        q(val),
        q(var)
    );
    let ids = if id_columns.is_empty() { "—".to_string() } else { id_columns.join(", ") };
    Ok((sql, StepLabel::new("step.melt", &[("id_columns", &ids), ("count", &values.len().to_string())])))
}

pub(crate) fn transpose(ctx: &OpContext, header_column: Option<&str>) -> Result<(String, StepLabel)> {
    let header = header_column.map(str::trim).filter(|h| !h.is_empty());
    if let Some(h) = header {
        ctx.require(h)?;
    }
    let cur = ctx.current_query;
    let count = (ctx.query)(&format!("SELECT COUNT(*) FROM ({cur})"))?
        .first()
        .and_then(|r| r.first().cloned().flatten())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    if count > TRANSPOSE_MAX_ROWS {
        return Err(CoreError::invalid(format!(
            "Transpose works on tables of up to {TRANSPOSE_MAX_ROWS} rows; this one has {count}. Filter or group it first."
        )));
    }
    if count == 0 {
        return Err(CoreError::invalid("The table has no rows"));
    }
    let names: Vec<String> = match header {
        Some(h) => {
            let rows = (ctx.query)(&format!(
                "SELECT CAST({} AS VARCHAR) FROM ({}) ORDER BY {}",
                q(h),
                with_row_number(cur),
                q(ROW_NUMBER)
            ))?;
            rows.into_iter().map(|r| r.into_iter().next().flatten().unwrap_or_else(|| "NULL".to_string())).collect()
        }
        None => (1..=count).map(|i| format!("row_{i}")).collect(),
    };
    let first = "column";
    let mut used = vec![first.to_string()];
    let out_names: Vec<String> = names
        .iter()
        .map(|n| {
            let u = unique_name(&used, n);
            used.push(u.clone());
            u
        })
        .collect();
    let rn = q(ROW_NUMBER);
    let src = with_row_number(cur);
    let parts: Vec<String> = ctx
        .columns
        .iter()
        .filter(|c| Some(c.name.as_str()) != header)
        .enumerate()
        .map(|(pos, c)| {
            let cells: Vec<String> = out_names
                .iter()
                .enumerate()
                .map(|(i, n)| format!("FIRST(CAST({} AS VARCHAR)) FILTER (WHERE {rn} = {}) AS {}", q(&c.name), i + 1, q(n)))
                .collect();
            format!("SELECT {pos} AS \"__pv_pos\", {} AS {}, {} FROM __pv_src", quote_literal(&c.name), q(first), cells.join(", "))
        })
        .collect();
    if parts.is_empty() {
        return Err(CoreError::invalid("Nothing to transpose besides the header column"));
    }
    let sql = format!(
        "WITH __pv_src AS ({src}) SELECT * EXCLUDE (\"__pv_pos\") FROM ({}) ORDER BY \"__pv_pos\"",
        parts.join(" UNION ALL ")
    );
    Ok((sql, StepLabel::new("step.transpose", &[("header", header.unwrap_or("—"))])))
}

pub(crate) fn value_counts(ctx: &OpContext, columns: &[String], normalize: bool, dropna: bool) -> Result<(String, StepLabel)> {
    if columns.is_empty() {
        return Err(CoreError::invalid("Choose at least one column"));
    }
    for c in columns {
        ctx.require(c)?;
    }
    let keys = key_list(columns);
    let count = unique_name(columns, "count");
    let mut items = vec![keys.clone(), format!("COUNT(*) AS {}", q(&count))];
    if normalize {
        let proportion = unique_name(&[columns.to_vec(), vec![count.clone()]].concat(), "proportion");
        items.push(format!("COUNT(*) / SUM(COUNT(*)) OVER () AS {}", q(&proportion)));
    }
    let filter = if dropna {
        format!(" WHERE {}", columns.iter().map(|c| format!("{} IS NOT NULL", q(c))).collect::<Vec<_>>().join(" AND "))
    } else {
        String::new()
    };
    let sql = format!(
        "SELECT {} FROM ({}){filter} GROUP BY {keys} ORDER BY {} DESC, {keys} NULLS LAST",
        items.join(", "),
        ctx.current_query,
        q(&count)
    );
    Ok((sql, StepLabel::new("step.value_counts", &[("columns", &columns.join(", "))])))
}

pub(crate) fn describe(ctx: &OpContext) -> Result<(String, StepLabel)> {
    Ok((format!("SELECT * FROM (SUMMARIZE SELECT * FROM ({}))", ctx.current_query), StepLabel::new("step.describe", &[])))
}
