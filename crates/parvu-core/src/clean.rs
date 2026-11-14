//! Filtering and cleaning operations: condition filters, filling missing
//! values, multi-column sort, choosing columns, head/tail/sample, splitting
//! a column and binning.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::ops::{OpContext, ROW_NUMBER, StepLabel, fmt_number, typed_literal, unique_name, with_row_number};
use crate::page::ColumnMeta;
use crate::sql::{quote_ident as q, quote_literal};

// ---------------------------------------------------------------------------
// Filter
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CondOp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Between,
    Contains,
    NotContains,
    StartsWith,
    EndsWith,
    /// Regular expression search (partial match).
    Matches,
    /// One of a comma-separated list.
    In,
    NotIn,
    IsNull,
    NotNull,
    /// NULL or empty/blank text.
    IsEmpty,
    NotEmpty,
}

impl CondOp {
    pub fn name(self) -> &'static str {
        match self {
            CondOp::Eq => "=",
            CondOp::Ne => "≠",
            CondOp::Gt => ">",
            CondOp::Ge => "≥",
            CondOp::Lt => "<",
            CondOp::Le => "≤",
            CondOp::Between => "between",
            CondOp::Contains => "contains",
            CondOp::NotContains => "does not contain",
            CondOp::StartsWith => "starts with",
            CondOp::EndsWith => "ends with",
            CondOp::Matches => "matches",
            CondOp::In => "in",
            CondOp::NotIn => "not in",
            CondOp::IsNull => "is null",
            CondOp::NotNull => "is not null",
            CondOp::IsEmpty => "is empty",
            CondOp::NotEmpty => "is not empty",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Condition {
    pub column: String,
    pub op: CondOp,
    #[serde(default)]
    pub value: String,
    /// Upper bound for `between`.
    #[serde(default)]
    pub value2: String,
    /// For the text comparisons (contains, starts/ends with, matches, =, ≠ on text).
    #[serde(default)]
    pub case_sensitive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Combine {
    #[default]
    And,
    Or,
}

fn is_text(meta: &ColumnMeta) -> bool {
    let t = meta.dtype.to_uppercase();
    t == "VARCHAR" || t.starts_with("VARCHAR(") || t == "TEXT" || t == "STRING"
}

fn condition_sql(ctx: &OpContext, c: &Condition) -> Result<String> {
    let meta = ctx.column(&c.column)?;
    let col = q(&c.column);
    let text = format!("CAST({col} AS VARCHAR)");
    // Text comparisons fold case unless asked not to.
    let fold = |s: String| if c.case_sensitive { s } else { format!("LOWER({s})") };
    let text_lit = |v: &str| {
        let lit = quote_literal(v);
        if c.case_sensitive { lit } else { format!("LOWER({lit})") }
    };
    let lit = |v: &str| typed_literal(v, &meta.dtype);
    let need = |v: &str| {
        if v.trim().is_empty() && !is_text(meta) {
            Err(CoreError::invalid(format!("Enter a value for \"{}\"", c.column)))
        } else {
            Ok(())
        }
    };
    let list = |v: &str| -> Result<String> {
        let items: Vec<&str> = v.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        if items.is_empty() {
            return Err(CoreError::invalid(format!("Enter values separated by commas for \"{}\"", c.column)));
        }
        if is_text(meta) {
            Ok(items.iter().map(|s| text_lit(s)).collect::<Vec<_>>().join(", "))
        } else {
            Ok(items.iter().map(|s| lit(s)).collect::<Result<Vec<_>>>()?.join(", "))
        }
    };
    Ok(match c.op {
        CondOp::Eq | CondOp::Ne => {
            let op = if c.op == CondOp::Eq { "=" } else { "<>" };
            if is_text(meta) {
                format!("{} {op} {}", fold(col.clone()), text_lit(&c.value))
            } else {
                need(&c.value)?;
                format!("{col} {op} {}", lit(&c.value)?)
            }
        }
        CondOp::Gt | CondOp::Ge | CondOp::Lt | CondOp::Le => {
            need(&c.value)?;
            let op = match c.op {
                CondOp::Gt => ">",
                CondOp::Ge => ">=",
                CondOp::Lt => "<",
                _ => "<=",
            };
            format!("{col} {op} {}", lit(&c.value)?)
        }
        CondOp::Between => {
            need(&c.value)?;
            need(&c.value2)?;
            format!("{col} BETWEEN {} AND {}", lit(&c.value)?, lit(&c.value2)?)
        }
        CondOp::Contains => format!("contains({}, {})", fold(text.clone()), text_lit(&c.value)),
        CondOp::NotContains => format!("NOT contains({}, {})", fold(text.clone()), text_lit(&c.value)),
        CondOp::StartsWith => format!("starts_with({}, {})", fold(text.clone()), text_lit(&c.value)),
        CondOp::EndsWith => format!("ends_with({}, {})", fold(text.clone()), text_lit(&c.value)),
        CondOp::Matches => {
            let flags = if c.case_sensitive { "" } else { ", 'i'" };
            format!("regexp_matches({text}, {}{flags})", quote_literal(&c.value))
        }
        CondOp::In => {
            let target = if is_text(meta) { fold(col.clone()) } else { col.clone() };
            format!("{target} IN ({})", list(&c.value)?)
        }
        CondOp::NotIn => {
            let target = if is_text(meta) { fold(col.clone()) } else { col.clone() };
            format!("{target} NOT IN ({})", list(&c.value)?)
        }
        CondOp::IsNull => format!("{col} IS NULL"),
        CondOp::NotNull => format!("{col} IS NOT NULL"),
        CondOp::IsEmpty => format!("({col} IS NULL OR TRIM({text}) = '')"),
        CondOp::NotEmpty => format!("({col} IS NOT NULL AND TRIM({text}) <> '')"),
    })
}

pub(crate) fn filter(ctx: &OpContext, conditions: &[Condition], combine: Combine) -> Result<(String, StepLabel)> {
    if conditions.is_empty() {
        return Err(CoreError::invalid("Add at least one condition"));
    }
    let parts = conditions.iter().map(|c| condition_sql(ctx, c)).collect::<Result<Vec<_>>>()?;
    let joiner = match combine {
        Combine::And => " AND ",
        Combine::Or => " OR ",
    };
    let sql = format!("SELECT * FROM ({}) WHERE ({})", ctx.current_query, parts.join(&format!("){joiner}(")));
    let text: Vec<String> = conditions
        .iter()
        .map(|c| match c.op {
            CondOp::IsNull | CondOp::NotNull | CondOp::IsEmpty | CondOp::NotEmpty => format!("{} {}", c.column, c.op.name()),
            CondOp::Between => format!("{} between {} and {}", c.column, c.value, c.value2),
            _ => format!("{} {} {}", c.column, c.op.name(), c.value),
        })
        .collect();
    let mut label = text.join(joiner.to_lowercase().as_str());
    if label.chars().count() > 80 {
        label = format!("{}…", label.chars().take(80).collect::<String>());
    }
    Ok((sql, StepLabel::new("step.filter_rows", &[("conditions", &label)])))
}

// ---------------------------------------------------------------------------
// Fill missing values
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FillMethod {
    Value,
    Forward,
    Backward,
    Mean,
    Median,
    Mode,
}

impl FillMethod {
    fn name(self) -> &'static str {
        match self {
            FillMethod::Value => "value",
            FillMethod::Forward => "forward",
            FillMethod::Backward => "backward",
            FillMethod::Mean => "mean",
            FillMethod::Median => "median",
            FillMethod::Mode => "mode",
        }
    }
}

pub(crate) fn fill_null(ctx: &OpContext, columns: &[String], method: FillMethod, value: &str) -> Result<(String, StepLabel)> {
    let columns: Vec<String> = if columns.is_empty() { ctx.columns.iter().map(|c| c.name.clone()).collect() } else { columns.to_vec() };
    let rn = q(ROW_NUMBER);
    let mut replaced = Vec::new();
    for name in &columns {
        let meta = ctx.column(name)?;
        let c = q(name);
        let fill = match method {
            FillMethod::Value => {
                if is_text(meta) {
                    quote_literal(value)
                } else {
                    if value.trim().is_empty() {
                        return Err(CoreError::invalid("Enter the value to fill with"));
                    }
                    typed_literal(value, &meta.dtype)?
                }
            }
            FillMethod::Forward => {
                format!("LAST_VALUE({c} IGNORE NULLS) OVER (ORDER BY {rn} ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW)")
            }
            FillMethod::Backward => {
                format!("FIRST_VALUE({c} IGNORE NULLS) OVER (ORDER BY {rn} ROWS BETWEEN CURRENT ROW AND UNBOUNDED FOLLOWING)")
            }
            FillMethod::Mean => {
                if !meta.is_numeric() {
                    return Err(CoreError::invalid(format!("\"{name}\" is not numeric; mean needs numbers")));
                }
                format!("AVG({c}) OVER ()")
            }
            FillMethod::Median => format!("MEDIAN({c}) OVER ()"),
            FillMethod::Mode => format!("MODE({c}) OVER ()"),
        };
        // Keep the column's type where the fill value allows it (mean of integers stays fractional).
        let expr = if method == FillMethod::Mean { format!("COALESCE({c}, {fill})") } else { format!("COALESCE({c}, CAST({fill} AS {}))", safe_type(meta)) };
        replaced.push(format!("{expr} AS {c}"));
    }
    if replaced.is_empty() {
        return Err(CoreError::invalid("No columns"));
    }
    let sql = format!(
        "SELECT * EXCLUDE ({rn}) REPLACE ({}) FROM ({}) ORDER BY {rn}",
        replaced.join(", "),
        with_row_number(ctx.current_query)
    );
    let cols_text = if columns.len() == ctx.columns.len() { "*".to_string() } else { columns.join(", ") };
    let how = if method == FillMethod::Value { format!("'{value}'") } else { method.name().to_string() };
    Ok((sql, StepLabel::new("step.fill_null", &[("columns", &cols_text), ("method", &how)])))
}

/// The column's type for a CAST, or VARCHAR when its spelling is not safe to embed.
fn safe_type(meta: &ColumnMeta) -> &str {
    if crate::sql::is_safe_type_name(&meta.dtype) { &meta.dtype } else { "VARCHAR" }
}

// ---------------------------------------------------------------------------
// Sort by several columns, choose columns, head/tail/sample
// ---------------------------------------------------------------------------

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortKey {
    pub column: String,
    #[serde(default = "default_true")]
    pub ascending: bool,
    #[serde(default)]
    pub nulls_first: bool,
}

pub(crate) fn sort_by(ctx: &OpContext, keys: &[SortKey]) -> Result<(String, StepLabel)> {
    if keys.is_empty() {
        return Err(CoreError::invalid("Add at least one sort column"));
    }
    let mut parts = Vec::new();
    let mut text = Vec::new();
    for k in keys {
        ctx.require(&k.column)?;
        let dir = if k.ascending { "ASC" } else { "DESC" };
        let nulls = if k.nulls_first { "NULLS FIRST" } else { "NULLS LAST" };
        parts.push(format!("{} {dir} {nulls}", q(&k.column)));
        text.push(format!("{} {}", k.column, if k.ascending { "↑" } else { "↓" }));
    }
    Ok((
        format!("SELECT * FROM ({}) ORDER BY {}", ctx.current_query, parts.join(", ")),
        // Same key as the header sort, so a new sort replaces the previous one.
        StepLabel::new("step.sort_by", &[("columns", &text.join(", "))]),
    ))
}

pub(crate) fn select_columns(ctx: &OpContext, columns: &[String]) -> Result<(String, StepLabel)> {
    if columns.is_empty() {
        return Err(CoreError::invalid("Keep at least one column"));
    }
    for (i, c) in columns.iter().enumerate() {
        ctx.require(c)?;
        if columns[..i].contains(c) {
            return Err(CoreError::invalid(format!("\"{c}\" is listed twice")));
        }
    }
    let list = columns.iter().map(|c| q(c)).collect::<Vec<_>>().join(", ");
    Ok((
        format!("SELECT {list} FROM ({})", ctx.current_query),
        StepLabel::new("step.select_columns", &[("count", &columns.len().to_string()), ("total", &ctx.columns.len().to_string())]),
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SliceMode {
    Head,
    Tail,
    Sample,
}

pub(crate) fn slice(ctx: &OpContext, mode: SliceMode, n: u64, seed: Option<u64>) -> Result<(String, StepLabel)> {
    if n == 0 {
        return Err(CoreError::invalid("The number of rows must be at least 1"));
    }
    let cur = ctx.current_query;
    let rn = q(ROW_NUMBER);
    let (sql, word) = match mode {
        SliceMode::Head => (format!("SELECT * FROM ({cur}) LIMIT {n}"), "first"),
        SliceMode::Tail => (
            format!(
                "SELECT * EXCLUDE ({rn}) FROM ({}) QUALIFY {rn} > COUNT(*) OVER () - {n} ORDER BY {rn}",
                with_row_number(cur)
            ),
            "last",
        ),
        SliceMode::Sample => {
            let repeat = seed.map(|s| format!(" REPEATABLE ({s})")).unwrap_or_default();
            // Keep the sampled rows in table order.
            (
                format!(
                    "SELECT * EXCLUDE ({rn}) FROM (SELECT * FROM ({}) USING SAMPLE reservoir({n} ROWS){repeat}) ORDER BY {rn}",
                    with_row_number(cur)
                ),
                "random",
            )
        }
    };
    Ok((sql, StepLabel::new("step.slice", &[("mode", word), ("n", &n.to_string())])))
}

// ---------------------------------------------------------------------------
// Split a column
// ---------------------------------------------------------------------------

/// Most columns a split can produce.
pub const SPLIT_MAX_PARTS: u32 = 50;

pub(crate) fn split_column(
    ctx: &OpContext,
    column: &str,
    separator: &str,
    regex: bool,
    parts: Option<u32>,
    keep_original: bool,
) -> Result<(String, StepLabel)> {
    ctx.require(column)?;
    if separator.is_empty() {
        return Err(CoreError::invalid("Enter a separator"));
    }
    let c = q(column);
    let split = if regex {
        format!("string_split_regex(CAST({c} AS VARCHAR), {})", quote_literal(separator))
    } else {
        format!("string_split(CAST({c} AS VARCHAR), {})", quote_literal(separator))
    };
    let n = match parts {
        Some(n) => n,
        None => (ctx.query)(&format!("SELECT MAX(LEN({split})) FROM ({})", ctx.current_query))?
            .first()
            .and_then(|r| r.first().cloned().flatten())
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1),
    }
    .clamp(1, SPLIT_MAX_PARTS);
    let mut used: Vec<String> = ctx.columns.iter().map(|m| m.name.clone()).filter(|m| keep_original || m != column).collect();
    let mut new_cols = Vec::new();
    for i in 1..=n {
        let name = unique_name(&used, &format!("{column}_{i}"));
        used.push(name.clone());
        new_cols.push(format!("{split}[{i}] AS {}", q(&name)));
    }
    // New columns go right after the original.
    let mut items = Vec::new();
    for m in ctx.columns {
        if m.name == column {
            if keep_original {
                items.push(q(&m.name));
            }
            items.extend(new_cols.iter().cloned());
        } else {
            items.push(q(&m.name));
        }
    }
    Ok((
        format!("SELECT {} FROM ({})", items.join(", "), ctx.current_query),
        StepLabel::new("step.split_column", &[("column", column), ("separator", separator), ("count", &n.to_string())]),
    ))
}

// ---------------------------------------------------------------------------
// Binning
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BinMethod {
    /// `bins` intervals of equal width between the minimum and maximum (pandas `cut`).
    EqualWidth,
    /// `bins` intervals holding about the same number of rows (pandas `qcut`).
    Quantile,
    /// Explicit edges.
    Edges,
}

fn first_row_numbers(ctx: &OpContext, sql: &str) -> Result<Vec<Option<f64>>> {
    let rows = (ctx.query)(sql)?;
    Ok(rows.into_iter().next().unwrap_or_default().into_iter().map(|c| c.and_then(|s| s.parse::<f64>().ok())).collect())
}

pub(crate) fn bin(
    ctx: &OpContext,
    column: &str,
    method: BinMethod,
    bins: u32,
    edges: &[f64],
    labels: &[String],
    output: &str,
) -> Result<(String, StepLabel)> {
    let meta = ctx.column(column)?;
    if !meta.is_numeric() {
        return Err(CoreError::invalid(format!("\"{column}\" is not numeric; binning needs numbers")));
    }
    if output.trim().is_empty() {
        return Err(CoreError::invalid("Output column name is empty"));
    }
    let c = q(column);
    let x = format!("CAST({c} AS DOUBLE)");
    let cur = ctx.current_query;
    let mut e: Vec<f64> = match method {
        BinMethod::Edges => {
            let mut v = edges.to_vec();
            if v.iter().any(|f| !f.is_finite()) {
                return Err(CoreError::invalid("Edges must be numbers"));
            }
            v.sort_by(f64::total_cmp);
            v
        }
        BinMethod::EqualWidth | BinMethod::Quantile => {
            if !(1..=1000).contains(&bins) {
                return Err(CoreError::invalid("Choose between 1 and 1000 bins"));
            }
            if method == BinMethod::EqualWidth {
                let mm = first_row_numbers(ctx, &format!("SELECT MIN({x}), MAX({x}) FROM ({cur})"))?;
                let (Some(Some(lo)), Some(Some(hi))) = (mm.first().copied(), mm.get(1).copied()) else {
                    return Err(CoreError::invalid(format!("\"{column}\" has no values")));
                };
                if lo == hi {
                    vec![lo, hi]
                } else {
                    (0..=bins).map(|i| if i == bins { hi } else { lo + (hi - lo) * f64::from(i) / f64::from(bins) }).collect()
                }
            } else {
                let qs: Vec<String> =
                    (0..=bins).map(|i| format!("QUANTILE_CONT({x}, {})", fmt_number(f64::from(i) / f64::from(bins)))).collect();
                let vals = first_row_numbers(ctx, &format!("SELECT {} FROM ({cur})", qs.join(", ")))?;
                let v: Vec<f64> = vals.into_iter().flatten().collect();
                if v.is_empty() {
                    return Err(CoreError::invalid(format!("\"{column}\" has no values")));
                }
                v
            }
        }
    };
    e.dedup();
    if e.len() < 2 && !(e.len() == 1 && method != BinMethod::Edges) {
        return Err(CoreError::invalid("Give at least two edges"));
    }
    if e.len() == 1 {
        e.push(e[0]);
    }
    let n = e.len() - 1;
    if !labels.is_empty() && labels.len() != n {
        return Err(CoreError::invalid(format!("Give {n} labels, one per bin, or none")));
    }
    let mut whens = Vec::new();
    for i in 0..n {
        let (lo, hi) = (e[i], e[i + 1]);
        let label = labels.get(i).cloned().unwrap_or_else(|| {
            let open = if i == 0 { "[" } else { "(" };
            format!("{open}{}, {}]", fmt_number(lo), fmt_number(hi))
        });
        let lower = if i == 0 { format!("{x} >= {}", fmt_number(lo)) } else { format!("{x} > {}", fmt_number(lo)) };
        whens.push(format!("WHEN {lower} AND {x} <= {} THEN {}", fmt_number(hi), quote_literal(&label)));
    }
    let case = format!("CASE {} END", whens.join(" "));
    let out = output.trim();
    let sql = if ctx.columns.iter().any(|m| m.name == out) {
        format!("SELECT * REPLACE ({case} AS {}) FROM ({cur})", q(out))
    } else {
        format!("SELECT *, {case} AS {} FROM ({cur})", q(out))
    };
    Ok((sql, StepLabel::new("step.bin", &[("column", column), ("count", &n.to_string())])))
}
