//! Table operations: one definition per operation.
//!
//! Each [`Operation`] builds the SQL that wraps the tab's current query, plus
//! a translatable [`StepLabel`] for the applied-steps panel. The UI's menus,
//! dialogs and column context menu all send these, and the planned AI tools
//! will use the same enum.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::clean::{BinMethod, Combine, Condition, FillMethod, RowFilter, SliceMode, SortKey, label_where, only_where};
use crate::colfn::{ColumnFn, JsonKey, Output, RelativeRange, TimeUnit};
use crate::error::{CoreError, Result};
use crate::page::{Cell, ColumnMeta};
use crate::reshape::{AggFunc, Aggregation};
use crate::sql::{is_safe_type_name, quote_ident, quote_literal};
use crate::types::{Reading, TypeFamily};
use crate::window::WindowSpec;

/// i18n key plus named arguments, translated by the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepLabel {
    pub key: String,
    pub args: BTreeMap<String, String>,
}

impl StepLabel {
    pub fn new(key: &str, args: &[(&str, &str)]) -> Self {
        Self {
            key: key.to_string(),
            args: args.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Keep {
    /// Keep the first row of each group of duplicates.
    #[default]
    First,
    /// Keep the last row.
    Last,
    /// Keep none: every row that has a duplicate goes (pandas `keep=False`).
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum JoinType {
    Inner,
    Left,
    Right,
    FullOuter,
}

impl JoinType {
    pub fn sql(self) -> &'static str {
        match self {
            JoinType::Inner => "INNER",
            JoinType::Left => "LEFT",
            JoinType::Right => "RIGHT",
            JoinType::FullOuter => "FULL OUTER",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Operation {
    /// Sort rows by one column.
    Sort { column: String, ascending: bool },
    /// Rename a column.
    Rename { column: String, new_name: String },
    /// Remove a column.
    Remove { column: String },
    /// Cast a column to a DuckDB type such as `INTEGER`, `DOUBLE`, `DATE`,
    /// `TIMESTAMP`, `VARCHAR`, `BOOLEAN` or `DECIMAL(18,2)`.
    ChangeType { column: String, new_type: String },
    /// Convert a column to another type, reading it as `to`: number formats
    /// (`1.234,5`), date formats (`%d.%m.%Y`), yes/no words, Unix time…
    /// Never fails: values that don't convert become empty, or `on_error`.
    Convert {
        column: String,
        to: Reading,
        /// Value for cells that don't convert; must be valid in the new type.
        #[serde(default)]
        on_error: Option<String>,
        /// Keep the original column, as `<column>_raw`, right after it.
        #[serde(default)]
        keep_original: bool,
    },
    /// Keep (`keep: true`) or remove the rows whose `column` doesn't convert
    /// as `to` (Power Query's Keep Errors / Remove Errors). Empty cells are
    /// not errors.
    ConversionErrors { column: String, to: Reading, keep: bool },
    /// A function of one column (trim, change case, extract text, invert a
    /// yes/no…), in place or into a new column right after it.
    ColumnTransform {
        column: String,
        func: ColumnFn,
        #[serde(default)]
        output: Output,
        /// Only change these rows; the others keep their value (a new
        /// column is empty there).
        #[serde(default, rename = "where", skip_serializing_if = "Option::is_none")]
        filter: Option<RowFilter>,
    },
    /// Turn chosen keys of a JSON object column into columns (typed by
    /// `keys`), at the column's place; keys missing from a row give empty
    /// cells. The keys usually come from a sample (`Workspace::json_keys`).
    ExpandJson {
        column: String,
        keys: Vec<JsonKey>,
        /// Keep the JSON column before the new ones.
        #[serde(default)]
        keep_original: bool,
    },
    /// Turn the fields of a struct column into columns, at its place.
    ExpandStruct { column: String },
    /// One row per item of a list or JSON array column; rows with an empty
    /// or missing list are kept, with an empty cell.
    ToRows { column: String },
    /// One row per piece of `column` split by `delimiter` ("a, b" → two
    /// rows); empty cells stay one row.
    SplitToRows {
        column: String,
        delimiter: String,
        /// Remove spaces around each piece.
        #[serde(default)]
        trim: bool,
    },
    /// Whole `unit`s from `start` to `end` (two date columns), into the new
    /// column `name` right after `end`.
    DateDiff { start: String, end: String, unit: TimeUnit, name: String },
    /// Keep the rows whose date falls in a range relative to today ("last 7
    /// days", "this month"…). Today is when the query runs.
    DateRange { column: String, range: RelativeRange },
    /// Join the text of several columns with `separator` into the new
    /// column `name`, right after the first of them; empty cells are skipped.
    MergeColumns {
        columns: Vec<String>,
        #[serde(default)]
        separator: String,
        name: String,
        /// Keep the merged columns (otherwise they are removed).
        #[serde(default = "yes")]
        keep_columns: bool,
    },
    /// Add a copy of a column.
    Duplicate { column: String },
    /// Remove duplicate rows, comparing the given columns (pandas `drop_duplicates`).
    DropDuplicates {
        /// Empty means all columns.
        #[serde(default)]
        columns: Vec<String>,
        #[serde(default)]
        keep: Keep,
    },
    /// Add a yes/no column that is true for duplicated rows (pandas
    /// `duplicated()`): every occurrence but the first (`keep: first`), but
    /// the last, or every occurrence (`none`). Empty `columns`: all columns.
    MarkDuplicates {
        #[serde(default)]
        columns: Vec<String>,
        #[serde(default)]
        keep: Keep,
        name: String,
    },
    /// Remove rows where a column is missing (pandas `dropna`).
    DropNull {
        column: String,
        /// `None` drops SQL NULLs; otherwise drops rows equal to this sentinel.
        #[serde(default)]
        null_value: Option<String>,
    },
    /// Keep rows whose value (as text) is one of `values`; `None` matches NULL.
    FilterValues { column: String, values: Vec<Cell> },
    /// Join another open table (`right`, by table name) on one key column each.
    Join { right: String, join_type: JoinType, left_key: String, right_key: String },
    /// Append the rows of other open tables (by name) below this one,
    /// matching columns by name; `distinct` removes duplicate rows (SQL UNION).
    Append { tables: Vec<String>, distinct: bool },
    /// Add (or overwrite) a column computed by a SQL expression, e.g. a math
    /// operation or a compiled DSL expression. The current result is aliased
    /// as the table's name, so `"table"."col"` references work.
    AddColumn { name: String, expression: String },
    /// Replace text in `column`, writing to `output_column`. `expression` is
    /// the compiled replace expression for `column` (from the DSL crate).
    Replace {
        column: String,
        pattern: String,
        output_column: String,
        expression: String,
        /// Only replace in these rows.
        #[serde(default, rename = "where", skip_serializing_if = "Option::is_none")]
        filter: Option<RowFilter>,
    },
    /// Set `column` to `value` (empty when `None`) on the rows `where`
    /// selects (pandas `df.loc[condition, column] = value`).
    SetValue {
        column: String,
        #[serde(default)]
        value: Option<String>,
        #[serde(rename = "where")]
        filter: RowFilter,
    },

    // Reshaping (crate::reshape)
    /// One row per group with the aggregations (pandas `groupby().agg()`).
    GroupBy { by: Vec<String>, aggregations: Vec<Aggregation> },
    /// One output column per distinct value of `columns` (pandas `pivot_table`).
    Pivot {
        #[serde(default)]
        index: Vec<String>,
        columns: String,
        /// `None` counts rows (a crosstab).
        #[serde(default)]
        values: Option<String>,
        func: AggFunc,
    },
    /// Columns to rows (pandas `melt`). Empty `value_columns` means all others.
    Melt {
        #[serde(default)]
        id_columns: Vec<String>,
        #[serde(default)]
        value_columns: Vec<String>,
        variable_name: String,
        value_name: String,
    },
    /// Rows to columns and columns to rows (pandas `df.T`).
    Transpose {
        /// Column whose values name the new columns.
        #[serde(default)]
        header_column: Option<String>,
    },
    /// Count rows per distinct combination of `columns`, most frequent first
    /// (pandas `value_counts`); `normalize` gives proportions.
    ValueCounts {
        columns: Vec<String>,
        #[serde(default)]
        normalize: bool,
        #[serde(default)]
        dropna: bool,
    },
    /// One row of statistics per column (pandas `describe`).
    Describe,

    // Filtering and cleaning (crate::clean)
    /// Keep rows matching the conditions (pandas boolean indexing / `query`).
    Filter {
        conditions: Vec<Condition>,
        #[serde(default)]
        combine: Combine,
    },
    /// Fill missing values (pandas `fillna`, `ffill`, `bfill`); `value` is used
    /// with the `value` method.
    FillNull {
        /// Empty means all columns.
        #[serde(default)]
        columns: Vec<String>,
        method: FillMethod,
        #[serde(default)]
        value: String,
        /// Only fill in these rows.
        #[serde(default, rename = "where", skip_serializing_if = "Option::is_none")]
        filter: Option<RowFilter>,
    },
    /// Sort rows by several columns, first key first.
    SortBy { keys: Vec<SortKey> },
    /// Keep these columns, in this order.
    SelectColumns { columns: Vec<String> },
    /// Keep the first, last or a random sample of `n` rows (pandas `head`,
    /// `tail`, `sample`).
    Slice {
        mode: SliceMode,
        n: u64,
        #[serde(default)]
        seed: Option<u64>,
    },
    /// Split a text column into several columns at a separator (pandas
    /// `str.split(expand=True)`).
    SplitColumn {
        column: String,
        separator: String,
        #[serde(default)]
        regex: bool,
        /// `None` uses the largest number of parts found.
        #[serde(default)]
        parts: Option<u32>,
        #[serde(default)]
        keep_original: bool,
    },
    /// Put a numeric column's values into intervals, written to `output` (pandas
    /// `cut` / `qcut`). Use `bins` with `equal_width` or `quantile`, `edges` with
    /// `edges`; `labels` optionally name the intervals.
    Bin {
        column: String,
        method: BinMethod,
        #[serde(default)]
        bins: u32,
        #[serde(default)]
        edges: Vec<f64>,
        #[serde(default)]
        labels: Vec<String>,
        output: String,
    },

    // Column functions (crate::window)
    /// Running totals, shifts, differences, ranks and rolling values, optionally
    /// per group (pandas `cumsum`, `shift`, `diff`, `rank`, `rolling`).
    Window(WindowSpec),

    // Custom tools (crate::custom)
    /// A custom row function the user defined (Custom tools), by name, with
    /// its arguments by parameter name; written in place of its first
    /// column argument or into a new column right after it.
    CustomFunction {
        name: String,
        #[serde(default)]
        args: BTreeMap<String, crate::custom::CustomArg>,
        #[serde(default)]
        output: Output,
        /// Only change these rows.
        #[serde(default, rename = "where", skip_serializing_if = "Option::is_none")]
        filter: Option<RowFilter>,
    },
    /// A custom operation the user defined (Custom tools), by name, with its
    /// arguments by parameter name. `new_table` must match the definition:
    /// true when it opens its result as a new table.
    CustomOperation {
        name: String,
        #[serde(default)]
        args: BTreeMap<String, crate::custom::CustomArg>,
        #[serde(default)]
        new_table: bool,
    },
}

/// Row-number column added internally to keep or use table order.
pub const ROW_NUMBER: &str = "__pv_rn";

/// `SELECT *, ROW_NUMBER() OVER () AS "__pv_rn" FROM (<query>)`.
pub fn with_row_number(query: &str) -> String {
    format!("SELECT *, ROW_NUMBER() OVER () AS {} FROM ({query})", quote_ident(ROW_NUMBER))
}

/// `wanted`, or `wanted_2`, `wanted_3`… if it is already taken.
fn yes() -> bool {
    true
}

pub fn unique_name(taken: &[String], wanted: &str) -> String {
    let wanted = if wanted.is_empty() { "column" } else { wanted };
    if !taken.iter().any(|t| t == wanted) {
        return wanted.to_string();
    }
    (2..).map(|n| format!("{wanted}_{n}")).find(|n| !taken.iter().any(|t| t == n)).unwrap_or_default()
}

/// A number as SQL text: integers without a fraction, others in full precision.
pub fn fmt_number(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e15 { format!("{}", f as i64) } else { format!("{f:?}") }
}

/// SQL literal for a user-typed value compared with a column of type `dtype`:
/// numbers must parse, other types are cast from text.
pub fn typed_literal(raw: &str, dtype: &str) -> Result<String> {
    let v = raw.trim();
    let meta = ColumnMeta::new("", dtype);
    if meta.is_numeric() {
        if v.parse::<f64>().is_ok_and(f64::is_finite) {
            Ok(v.to_string())
        } else {
            Err(CoreError::invalid(format!("'{raw}' is not a number")))
        }
    } else if is_safe_type_name(dtype) && !dtype.eq_ignore_ascii_case("VARCHAR") {
        Ok(format!("CAST({} AS {dtype})", quote_literal(v)))
    } else {
        Ok(quote_literal(raw))
    }
}

/// Values a type change checks before it applies.
const CHANGE_TYPE_SAMPLE: u64 = 10_000;

/// What an operation needs to know about the tab it runs on.
pub struct OpContext<'a> {
    pub table: &'a str,
    pub current_query: &'a str,
    pub columns: &'a [ColumnMeta],
    /// Current query of another tab by name, for joins and appends.
    pub other_query: &'a dyn Fn(&str) -> Option<String>,
    /// Runs a read-only query and returns its rows as text, for operations
    /// that depend on the data (pivot values, bin edges, split widths).
    pub query: &'a dyn Fn(&str) -> Result<Vec<Vec<Cell>>>,
    /// A custom function or operation by name (the user's library).
    pub custom: &'a dyn Fn(&str) -> Option<std::sync::Arc<crate::custom::Definition>>,
}

/// An [`OpContext::custom`] that knows no custom tools.
pub fn no_custom(_: &str) -> Option<std::sync::Arc<crate::custom::Definition>> {
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltOp {
    pub sql: String,
    pub label: StepLabel,
}

impl<'a> OpContext<'a> {
    pub(crate) fn require(&self, column: &str) -> Result<()> {
        self.column(column).map(|_| ())
    }

    pub(crate) fn column(&self, column: &str) -> Result<&ColumnMeta> {
        self.columns.iter().find(|c| c.name == column).ok_or_else(|| CoreError::UnknownColumn(column.to_string()))
    }

    fn other(&self, name: &str) -> Result<String> {
        (self.other_query)(name).ok_or_else(|| CoreError::invalid(format!("No open table named {name}")))
    }

    /// `SELECT <list> FROM (<current>)` where `f` maps each column to its select item.
    fn select_each(&self, f: impl Fn(&str) -> String) -> String {
        let list: Vec<String> = self.columns.iter().map(|c| f(&c.name)).collect();
        format!("SELECT {} FROM ({})", list.join(", "), self.current_query)
    }
}

/// `(converted, failed)` expressions for reading `column` as `to`: the
/// converted value, and a condition true where a non-empty value doesn't
/// convert.
pub(crate) fn conversion(ctx: &OpContext, column: &str, to: &Reading) -> Result<(String, String)> {
    let meta = ctx.column(column)?;
    if !is_safe_type_name(to.dtype()) {
        return Err(CoreError::invalid(format!("Invalid type: {}", to.dtype())));
    }
    let x = quote_ident(column);
    let text = meta.family == TypeFamily::Text;
    let conv = to.expr_of(&x, text);
    let empty = if text { format!(" AND {x} <> ''") } else { String::new() };
    Ok((conv.clone(), format!("({x} IS NOT NULL{empty} AND {conv} IS NULL)")))
}

/// The columns that decide whether two rows are duplicates; empty means all.
fn duplicate_subset(ctx: &OpContext, columns: &[String]) -> Result<Vec<String>> {
    let subset: Vec<String> = if columns.is_empty() {
        ctx.columns.iter().map(|c| c.name.clone()).collect()
    } else {
        for c in columns {
            ctx.require(c)?;
        }
        columns.to_vec()
    };
    if subset.is_empty() {
        return Err(CoreError::invalid("No columns"));
    }
    Ok(subset)
}

/// SQL literal for a user-typed sentinel: numbers stay bare, anything else is quoted.
fn sentinel_literal(raw: &str) -> String {
    let t = raw.trim();
    if !t.is_empty() && (t.parse::<i64>().is_ok() || t.parse::<f64>().is_ok_and(|f| f.is_finite())) {
        t.to_string()
    } else {
        quote_literal(raw)
    }
}

impl Operation {
    /// Operations whose result is a different table (grouped, pivoted,
    /// joined…). They open their result in a new tab instead of changing
    /// the table they run on.
    pub fn is_table_op(&self) -> bool {
        matches!(
            self,
            Operation::GroupBy { .. }
                | Operation::Pivot { .. }
                | Operation::Melt { .. }
                | Operation::Transpose { .. }
                | Operation::ValueCounts { .. }
                | Operation::Describe
                | Operation::Join { .. }
                | Operation::Append { .. }
                | Operation::CustomOperation { new_table: true, .. }
        )
    }

    /// What a table operation's result is called after its source table:
    /// `by_region`, `pivot`, `join_customers`…
    pub fn result_suffix(&self) -> String {
        match self {
            Operation::GroupBy { by, .. } if by.is_empty() => "summary".into(),
            Operation::GroupBy { by, .. } => format!("by_{}", by.join("_")),
            Operation::Pivot { .. } => "pivot".into(),
            Operation::Melt { .. } => "unpivoted".into(),
            Operation::Transpose { .. } => "transposed".into(),
            Operation::ValueCounts { columns, .. } => format!("{}_counts", columns.join("_")),
            Operation::Describe => "described".into(),
            Operation::Join { right, .. } => format!("join_{right}"),
            Operation::Append { .. } => "append".into(),
            Operation::CustomOperation { name, .. } => name.clone(),
            _ => "result".into(),
        }
    }

    pub fn build(&self, ctx: &OpContext) -> Result<BuiltOp> {
        let cur = ctx.current_query;
        let q = quote_ident;
        let (sql, label) = match self {
            Operation::Sort { column, ascending } => {
                ctx.require(column)?;
                let dir = if *ascending { "ASC" } else { "DESC" };
                let word = if *ascending { "ascending" } else { "descending" };
                (
                    format!("SELECT * FROM ({cur}) ORDER BY {} {dir}", q(column)),
                    StepLabel::new("step.sort", &[("column", column), ("direction", word)]),
                )
            }
            Operation::Rename { column, new_name } => {
                ctx.require(column)?;
                if new_name.trim().is_empty() {
                    return Err(CoreError::invalid("New column name is empty"));
                }
                if new_name != column && ctx.columns.iter().any(|c| &c.name == new_name) {
                    return Err(CoreError::invalid(format!("Column {new_name} already exists")));
                }
                (
                    ctx.select_each(|c| if c == column { format!("{} AS {}", q(c), q(new_name)) } else { q(c) }),
                    StepLabel::new("step.rename", &[("old", column), ("new", new_name)]),
                )
            }
            Operation::Remove { column } => {
                ctx.require(column)?;
                let remaining: Vec<String> = ctx.columns.iter().filter(|c| &c.name != column).map(|c| q(&c.name)).collect();
                if remaining.is_empty() {
                    return Err(CoreError::invalid("Cannot remove the only column."));
                }
                (format!("SELECT {} FROM ({cur})", remaining.join(", ")), StepLabel::new("step.remove", &[("column", column)]))
            }
            Operation::Convert { column, to, on_error, keep_original } => {
                let (conv, failed) = conversion(ctx, column, to)?;
                let value = match on_error {
                    Some(v) => {
                        // Checked now: inside the CASE it would only fail on
                        // the first row that needs it, pages later.
                        let fallback = format!("CAST({} AS {})", quote_literal(v), to.dtype());
                        (ctx.query)(&format!("SELECT {fallback}")).map_err(|_| {
                            CoreError::invalid(format!("\"{v}\" is not a valid {} value", to.dtype()))
                        })?;
                        format!("CASE WHEN {failed} THEN {fallback} ELSE {conv} END")
                    }
                    None => conv,
                };
                let taken: Vec<String> = ctx.columns.iter().map(|c| c.name.clone()).collect();
                let raw = unique_name(&taken, &format!("{column}_raw"));
                let items: Vec<String> = ctx
                    .columns
                    .iter()
                    .flat_map(|c| {
                        if c.name != *column {
                            vec![q(&c.name)]
                        } else if *keep_original {
                            vec![format!("{value} AS {}", q(column)), format!("{} AS {}", q(column), q(&raw))]
                        } else {
                            vec![format!("{value} AS {}", q(column))]
                        }
                    })
                    .collect();
                (
                    format!("SELECT {} FROM ({cur})", items.join(", ")),
                    StepLabel::new("step.convert", &[("column", column), ("type", to.dtype())]),
                )
            }
            Operation::ConversionErrors { column, to, keep } => {
                let (_, failed) = conversion(ctx, column, to)?;
                let cond = if *keep { failed } else { format!("NOT {failed}") };
                let key = if *keep { "step.keep_conversion_errors" } else { "step.remove_conversion_errors" };
                (format!("SELECT * FROM ({cur}) WHERE {cond}"), StepLabel::new(key, &[("column", column), ("type", to.dtype())]))
            }
            Operation::ColumnTransform { column, func, output, filter } => {
                let meta = ctx.column(column)?;
                let expr = func.expr(column, &meta.dtype)?;
                let in_place_expr = only_where(ctx, filter.as_ref(), &expr, &q(column))?;
                let new_expr = only_where(ctx, filter.as_ref(), &expr, "NULL")?;
                // A bad pattern would only fail on the first page that needs
                // it (neither validation nor the row count evaluates it).
                if let ColumnFn::RegexExtract { pattern, group } = func {
                    (ctx.query)(&format!("SELECT regexp_extract('', {}, {group})", quote_literal(pattern)))
                        .map_err(|e| CoreError::invalid(format!("Invalid pattern: {e}")))?;
                }
                if let ColumnFn::Matches { text, how: crate::colfn::MatchHow::Regex, .. } = func {
                    (ctx.query)(&format!("SELECT regexp_matches('', {})", quote_literal(text)))
                        .map_err(|e| CoreError::invalid(format!("Invalid pattern: {e}")))?;
                }
                if let ColumnFn::JsonPath { path, .. } = func {
                    (ctx.query)(&format!("SELECT json_extract('{{}}', {})", quote_literal(path)))
                        .map_err(|e| CoreError::invalid(format!("Invalid JSON path: {e}")))?;
                }
                let label = label_where(StepLabel::new(func.key(), &[("column", column)]), filter.as_ref());
                match output {
                    Output::InPlace => (
                        ctx.select_each(|c| if c == column { format!("{in_place_expr} AS {}", q(c)) } else { q(c) }),
                        label,
                    ),
                    Output::New { name } => {
                        let taken: Vec<String> = ctx.columns.iter().map(|c| c.name.clone()).collect();
                        let wanted = if name.trim().is_empty() { format!("{column}_{}", func.suffix()) } else { name.trim().to_string() };
                        let new = unique_name(&taken, &wanted);
                        let items: Vec<String> = ctx
                            .columns
                            .iter()
                            .flat_map(|c| {
                                if c.name == *column { vec![q(&c.name), format!("{new_expr} AS {}", q(&new))] } else { vec![q(&c.name)] }
                            })
                            .collect();
                        (format!("SELECT {} FROM ({cur})", items.join(", ")), label)
                    }
                }
            }
            Operation::ExpandJson { column, keys, keep_original } => {
                let fam = TypeFamily::of(&ctx.column(column)?.dtype);
                if !matches!(fam, TypeFamily::Json | TypeFamily::Text) {
                    return Err(CoreError::invalid(format!("\"{column}\" is not a JSON column")));
                }
                if keys.is_empty() {
                    return Err(CoreError::invalid("Choose at least one key"));
                }
                let c = q(column);
                let mut taken: Vec<String> =
                    ctx.columns.iter().map(|m| m.name.clone()).filter(|n| *keep_original || n != column).collect();
                let mut new_cols = Vec::new();
                for k in keys {
                    let path = quote_literal(&k.path());
                    let expr = match k.dtype.as_str() {
                        "JSON" => format!("json_extract({c}, {path})"),
                        "VARCHAR" => format!("json_extract_string({c}, {path})"),
                        "BIGINT" | "DOUBLE" | "BOOLEAN" => format!("TRY_CAST(json_extract_string({c}, {path}) AS {})", k.dtype),
                        other => return Err(CoreError::invalid(format!("Invalid type for a JSON key: {other}"))),
                    };
                    // The key's own name, or "column.key" when that is taken.
                    let wanted = if taken.contains(&k.name) { format!("{column}.{}", k.name) } else { k.name.clone() };
                    let name = unique_name(&taken, &wanted);
                    taken.push(name.clone());
                    new_cols.push(format!("{expr} AS {}", q(&name)));
                }
                let mut items = Vec::new();
                for m in ctx.columns {
                    if m.name != *column {
                        items.push(q(&m.name));
                        continue;
                    }
                    if *keep_original {
                        items.push(q(&m.name));
                    }
                    items.extend(new_cols.iter().cloned());
                }
                (
                    format!("SELECT {} FROM ({cur})", items.join(", ")),
                    StepLabel::new("step.expand_json", &[("column", column), ("count", &keys.len().to_string())]),
                )
            }
            Operation::ExpandStruct { column } => {
                let fam = TypeFamily::of(&ctx.column(column)?.dtype);
                if fam != TypeFamily::Struct {
                    return Err(CoreError::invalid(format!("\"{column}\" is not a struct column")));
                }
                let c = q(column);
                let fields: Vec<String> = (ctx.query)(&format!("SELECT column_name FROM (DESCRIBE SELECT {c}.* FROM ({cur}))"))?
                    .into_iter()
                    .filter_map(|r| r.into_iter().next().flatten())
                    .collect();
                let mut taken: Vec<String> = ctx.columns.iter().map(|m| m.name.clone()).filter(|n| n != column).collect();
                let mut items = Vec::new();
                for m in ctx.columns {
                    if m.name != *column {
                        items.push(q(&m.name));
                        continue;
                    }
                    for f in &fields {
                        let wanted = if taken.contains(f) { format!("{column}.{f}") } else { f.clone() };
                        let name = unique_name(&taken, &wanted);
                        taken.push(name.clone());
                        items.push(format!("struct_extract({c}, {}) AS {}", quote_literal(f), q(&name)));
                    }
                }
                (format!("SELECT {} FROM ({cur})", items.join(", ")), StepLabel::new("step.expand_struct", &[("column", column)]))
            }
            Operation::ToRows { column } => {
                let fam = TypeFamily::of(&ctx.column(column)?.dtype);
                let c = q(column);
                let list = match fam {
                    TypeFamily::List => c.clone(),
                    TypeFamily::Json | TypeFamily::Text => format!("try(from_json({c}, '[\"JSON\"]'))"),
                    _ => return Err(CoreError::invalid(format!("\"{column}\" is not a list or JSON column"))),
                };
                // unnest drops rows whose list is empty or missing; keep them
                // as one row with an empty cell.
                let items = ctx.select_each(|n| {
                    if n == column {
                        format!("unnest(CASE WHEN len({list}) > 0 THEN {list} ELSE [NULL] END) AS {c}")
                    } else {
                        q(n)
                    }
                });
                (items, StepLabel::new("step.to_rows", &[("column", column)]))
            }
            Operation::SplitToRows { column, delimiter, trim } => {
                ctx.require(column)?;
                if delimiter.is_empty() {
                    return Err(CoreError::invalid("Enter a delimiter"));
                }
                let c = q(column);
                let parts = format!("string_split(CAST({c} AS VARCHAR), {})", quote_literal(delimiter));
                let piece = if *trim { "trim(p)" } else { "p" };
                let list = format!("list_transform({parts}, lambda p: {piece})");
                let items = ctx.select_each(|n| {
                    if n == column {
                        format!("unnest(CASE WHEN {c} IS NULL THEN [NULL] ELSE {list} END) AS {c}")
                    } else {
                        q(n)
                    }
                });
                (items, StepLabel::new("step.split_to_rows", &[("column", column), ("delimiter", delimiter)]))
            }
            Operation::DateDiff { start, end, unit, name } => {
                for c in [start, end] {
                    let fam = TypeFamily::of(&ctx.column(c)?.dtype);
                    if !matches!(fam, TypeFamily::Date | TypeFamily::Timestamp | TypeFamily::TimestampTz) {
                        return Err(CoreError::invalid(format!("\"{c}\" is not a date column")));
                    }
                }
                let name = name.trim();
                if name.is_empty() {
                    return Err(CoreError::invalid("Enter a name for the new column"));
                }
                let taken: Vec<String> = ctx.columns.iter().map(|c| c.name.clone()).collect();
                let new = unique_name(&taken, name);
                let expr = format!(
                    "datesub('{}', CAST({} AS TIMESTAMP), CAST({} AS TIMESTAMP)) AS {}",
                    unit.sql(),
                    q(start),
                    q(end),
                    q(&new)
                );
                let items: Vec<String> = ctx
                    .columns
                    .iter()
                    .flat_map(|c| if c.name == *end { vec![q(&c.name), expr.clone()] } else { vec![q(&c.name)] })
                    .collect();
                (
                    format!("SELECT {} FROM ({cur})", items.join(", ")),
                    StepLabel::new(&format!("step.date_diff_{}", unit.sql()), &[("start", start), ("end", end), ("name", &new)]),
                )
            }
            Operation::DateRange { column, range } => {
                let fam = TypeFamily::of(&ctx.column(column)?.dtype);
                if !matches!(fam, TypeFamily::Date | TypeFamily::Timestamp | TypeFamily::TimestampTz) {
                    return Err(CoreError::invalid(format!("\"{column}\" is not a date column")));
                }
                let (from, to) = range.bounds()?;
                let c = q(column);
                // The unit is part of the key, so each language words it properly.
                let label = match range {
                    RelativeRange::Last { n, unit } => StepLabel::new(
                        &format!("step.date_range.last_{}", unit.sql()),
                        &[("column", column), ("n", &n.to_string())],
                    ),
                    RelativeRange::This { unit } => StepLabel::new(&format!("step.date_range.this_{}", unit.sql()), &[("column", column)]),
                    RelativeRange::Previous { unit } => {
                        StepLabel::new(&format!("step.date_range.previous_{}", unit.sql()), &[("column", column)])
                    }
                    RelativeRange::YearToDate => StepLabel::new("step.date_range.ytd", &[("column", column)]),
                };
                // A plain range on the column: Parquet row groups outside it are skipped.
                (format!("SELECT * FROM ({cur}) WHERE {c} >= {from} AND {c} < {to}"), label)
            }
            Operation::MergeColumns { columns, separator, name, keep_columns } => {
                if columns.len() < 2 {
                    return Err(CoreError::invalid("Choose at least two columns to merge"));
                }
                for c in columns {
                    ctx.require(c)?;
                }
                let name = name.trim();
                if name.is_empty() {
                    return Err(CoreError::invalid("Enter a name for the merged column"));
                }
                let taken: Vec<String> = ctx
                    .columns
                    .iter()
                    .map(|c| c.name.clone())
                    .filter(|c| *keep_columns || !columns.contains(c))
                    .collect();
                let new = unique_name(&taken, name);
                let parts: Vec<String> = columns.iter().map(|c| format!("CAST({} AS VARCHAR)", q(c))).collect();
                let expr = format!("concat_ws({}, {}) AS {}", quote_literal(separator), parts.join(", "), q(&new));
                let mut items = Vec::new();
                for c in ctx.columns {
                    let merged = columns.contains(&c.name);
                    if !merged || *keep_columns {
                        items.push(q(&c.name));
                    }
                    if c.name == columns[0] {
                        items.push(expr.clone());
                    }
                }
                (
                    format!("SELECT {} FROM ({cur})", items.join(", ")),
                    StepLabel::new("step.merge_columns", &[("columns", &columns.join(", ")), ("name", &new)]),
                )
            }
            Operation::ChangeType { column, new_type } => {
                ctx.require(column)?;
                if !is_safe_type_name(new_type) {
                    return Err(CoreError::invalid(format!("Invalid type: {new_type}")));
                }
                // Values that don't fit would silently become empty (a text
                // column read as INTEGER became all NULL). Check the first
                // rows and send the user to Convert…, which shows them.
                let c = q(column);
                let check = (ctx.query)(&format!(
                    "SELECT count(*) FILTER (WHERE {c} IS NOT NULL AND TRY_CAST({c} AS {new_type}) IS NULL), count({c}) \
                     FROM (SELECT {c} FROM ({cur}) LIMIT {CHANGE_TYPE_SAMPLE})"
                ))?;
                let bad = check.first().and_then(|r| r[0].as_deref()).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                let seen = check.first().and_then(|r| r[1].as_deref()).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                if bad > 0 {
                    return Err(CoreError::invalid(format!(
                        "{bad} of the first {seen} values of \"{column}\" can't be read as {new_type} and would become empty. \
                         Use Type → Convert… to see them, or to convert anyway."
                    )));
                }
                (
                    // TRY_CAST: a value that doesn't convert becomes NULL. A plain
                    // CAST would let the step through (the first page and the row
                    // count never touch that value) and then fail on a later
                    // page, a sort or an export.
                    ctx.select_each(|c| {
                        if c == column { format!("TRY_CAST({} AS {new_type}) AS {}", q(c), q(c)) } else { q(c) }
                    }),
                    StepLabel::new("step.change_type", &[("column", column), ("type", new_type)]),
                )
            }
            Operation::Duplicate { column } => {
                ctx.require(column)?;
                let mut name = format!("{column}_copy");
                let mut n = 2;
                while ctx.columns.iter().any(|c| c.name == name) {
                    name = format!("{column}_copy{n}");
                    n += 1;
                }
                (
                    format!("SELECT *, {} AS {} FROM ({cur})", q(column), q(&name)),
                    StepLabel::new("step.duplicate", &[("column", column)]),
                )
            }
            Operation::DropDuplicates { columns, keep } => {
                let subset = duplicate_subset(ctx, columns)?;
                let partition = subset.iter().map(|c| q(c)).collect::<Vec<_>>().join(", ");
                let keep_rows = match keep {
                    Keep::First => format!("ROW_NUMBER() OVER (PARTITION BY {partition} ORDER BY __pv_rn ASC) = 1"),
                    Keep::Last => format!("ROW_NUMBER() OVER (PARTITION BY {partition} ORDER BY __pv_rn DESC) = 1"),
                    Keep::None => format!("COUNT(*) OVER (PARTITION BY {partition}) = 1"),
                };
                let label = match keep {
                    Keep::None => StepLabel::new("step.drop_duplicates_all", &[("columns", &subset.join(", "))]),
                    Keep::First | Keep::Last => StepLabel::new(
                        "step.drop_duplicates",
                        &[("keep", if *keep == Keep::First { "first" } else { "last" }), ("columns", &subset.join(", "))],
                    ),
                };
                (
                    format!(
                        "SELECT * EXCLUDE (__pv_rn) FROM (SELECT *, ROW_NUMBER() OVER () AS __pv_rn FROM ({cur})) \
                         QUALIFY {keep_rows} ORDER BY __pv_rn"
                    ),
                    label,
                )
            }
            Operation::MarkDuplicates { columns, keep, name } => {
                let name = name.trim();
                if name.is_empty() {
                    return Err(CoreError::invalid("Name the new column"));
                }
                if ctx.columns.iter().any(|c| c.name == name) {
                    return Err(CoreError::invalid(format!("A column named {name} already exists")));
                }
                let subset = duplicate_subset(ctx, columns)?;
                let partition = subset.iter().map(|c| q(c)).collect::<Vec<_>>().join(", ");
                let flag = match keep {
                    Keep::First => format!("ROW_NUMBER() OVER (PARTITION BY {partition} ORDER BY __pv_rn ASC) > 1"),
                    Keep::Last => format!("ROW_NUMBER() OVER (PARTITION BY {partition} ORDER BY __pv_rn DESC) > 1"),
                    Keep::None => format!("COUNT(*) OVER (PARTITION BY {partition}) > 1"),
                };
                (
                    format!(
                        "SELECT * EXCLUDE (__pv_rn), {flag} AS {} FROM (SELECT *, ROW_NUMBER() OVER () AS __pv_rn FROM ({cur})) \
                         ORDER BY __pv_rn",
                        q(name)
                    ),
                    StepLabel::new("step.mark_duplicates", &[("columns", &subset.join(", ")), ("name", name)]),
                )
            }
            Operation::DropNull { column, null_value } => {
                ctx.require(column)?;
                let c = q(column);
                match null_value {
                    None => (
                        format!("SELECT * FROM ({cur}) WHERE {c} IS NOT NULL"),
                        StepLabel::new("step.drop_null", &[("column", column), ("value", "NULL")]),
                    ),
                    Some(v) => (
                        format!("SELECT * FROM ({cur}) WHERE {c} IS NULL OR {c} <> {}", sentinel_literal(v)),
                        StepLabel::new("step.drop_null", &[("column", column), ("value", v)]),
                    ),
                }
            }
            Operation::FilterValues { column, values } => {
                ctx.require(column)?;
                if values.is_empty() {
                    return Err(CoreError::invalid("Select at least one value"));
                }
                let c = q(column);
                let texts: Vec<String> = values.iter().flatten().map(|v| quote_literal(v)).collect();
                let mut conds = Vec::new();
                if !texts.is_empty() {
                    conds.push(format!("CAST({c} AS VARCHAR) IN ({})", texts.join(", ")));
                }
                if values.iter().any(Option::is_none) {
                    conds.push(format!("{c} IS NULL"));
                }
                (
                    format!("SELECT * FROM ({cur}) WHERE {}", conds.join(" OR ")),
                    StepLabel::new("step.filter", &[("column", column), ("count", &values.len().to_string())]),
                )
            }
            Operation::Join { right, join_type, left_key, right_key } => {
                ctx.require(left_key)?;
                if right == ctx.table {
                    return Err(CoreError::invalid("Choose another table to join with"));
                }
                let right_sql = ctx.other(right)?;
                let (l, r) = (q(ctx.table), q(right));
                (
                    format!(
                        "SELECT * FROM ({cur}) AS {l} {} JOIN ({right_sql}) AS {r} ON {l}.{} = {r}.{}",
                        join_type.sql(),
                        q(left_key),
                        q(right_key)
                    ),
                    StepLabel::new(
                        "step.join",
                        &[
                            ("right", right),
                            ("left_key", left_key),
                            ("right_key", right_key),
                            ("join_type", join_type.sql()),
                            ("left", ctx.table),
                        ],
                    ),
                )
            }
            Operation::Append { tables, distinct } => {
                if tables.is_empty() {
                    return Err(CoreError::invalid("Select at least one table to append"));
                }
                // BY NAME: columns are matched by name, not position (the
                // same columns in another order lined up wrongly), and a
                // column only one table has is empty in the other's rows.
                let keyword = if *distinct { "UNION BY NAME" } else { "UNION ALL BY NAME" };
                let mut parts = vec![format!("SELECT * FROM ({cur})")];
                for t in tables {
                    parts.push(format!("SELECT * FROM ({})", ctx.other(t)?));
                }
                (
                    parts.join(&format!(" {keyword} ")),
                    StepLabel::new("step.append", &[("tables", &tables.join(", ")), ("mode", keyword)]),
                )
            }
            Operation::AddColumn { name, expression } => {
                if name.trim().is_empty() {
                    return Err(CoreError::invalid("Column name is empty"));
                }
                let t = q(ctx.table);
                let sql = if ctx.columns.iter().any(|c| &c.name == name) {
                    format!("SELECT * REPLACE ({expression} AS {}) FROM ({cur}) AS {t}", q(name))
                } else {
                    format!("SELECT *, {expression} AS {} FROM ({cur}) AS {t}", q(name))
                };
                (sql, StepLabel::new("step.add_column", &[("column", name)]))
            }
            Operation::Replace { column, pattern, output_column, expression, filter } => {
                ctx.require(column)?;
                if output_column.trim().is_empty() {
                    return Err(CoreError::invalid("Output column name is empty"));
                }
                let t = q(ctx.table);
                let exists = ctx.columns.iter().any(|c| &c.name == output_column);
                let old = if exists { q(output_column) } else { "NULL".to_string() };
                let expr = only_where(ctx, filter.as_ref(), expression, &old)?;
                let sql = if exists {
                    format!("SELECT * REPLACE ({expr} AS {}) FROM ({cur}) AS {t}", q(output_column))
                } else {
                    format!("SELECT *, {expr} AS {} FROM ({cur}) AS {t}", q(output_column))
                };
                let label = StepLabel::new("step.replace", &[("column", column), ("pattern", pattern)]);
                (sql, label_where(label, filter.as_ref()))
            }
            Operation::GroupBy { by, aggregations } => crate::reshape::group_by(ctx, by, aggregations)?,
            Operation::Pivot { index, columns, values, func } => {
                crate::reshape::pivot(ctx, index, columns, values.as_deref(), *func)?
            }
            Operation::Melt { id_columns, value_columns, variable_name, value_name } => {
                crate::reshape::melt(ctx, id_columns, value_columns, variable_name, value_name)?
            }
            Operation::Transpose { header_column } => crate::reshape::transpose(ctx, header_column.as_deref())?,
            Operation::ValueCounts { columns, normalize, dropna } => {
                crate::reshape::value_counts(ctx, columns, *normalize, *dropna)?
            }
            Operation::Describe => crate::reshape::describe(ctx)?,
            Operation::Filter { conditions, combine } => crate::clean::filter(ctx, conditions, *combine)?,
            Operation::FillNull { columns, method, value, filter } => {
                crate::clean::fill_null(ctx, columns, *method, value, filter.as_ref())?
            }
            Operation::SetValue { column, value, filter } => crate::clean::set_value(ctx, column, value.as_deref(), filter)?,
            Operation::SortBy { keys } => crate::clean::sort_by(ctx, keys)?,
            Operation::SelectColumns { columns } => crate::clean::select_columns(ctx, columns)?,
            Operation::Slice { mode, n, seed } => crate::clean::slice(ctx, *mode, *n, *seed)?,
            Operation::SplitColumn { column, separator, regex, parts, keep_original } => {
                crate::clean::split_column(ctx, column, separator, *regex, *parts, *keep_original)?
            }
            Operation::Bin { column, method, bins, edges, labels, output } => {
                crate::clean::bin(ctx, column, *method, *bins, edges, labels, output)?
            }
            Operation::Window(spec) => crate::window::window(ctx, spec)?,
            Operation::CustomFunction { name, args, output, filter } => {
                crate::custom::fill::build_function(ctx, name, args, output, filter.as_ref())?
            }
            Operation::CustomOperation { name, args, new_table } => {
                crate::custom::fill::build_operation(ctx, name, args, *new_table)?
            }
        };
        Ok(BuiltOp { sql, label })
    }
}

/// JSON schema of [`Operation`], for AI and MCP tool definitions.
#[cfg(feature = "schema")]
pub fn operation_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(Operation)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_query(_: &str) -> Result<Vec<Vec<Cell>>> {
        Ok(Vec::new())
    }

    fn cols() -> Vec<ColumnMeta> {
        vec![ColumnMeta::new("id", "BIGINT"), ColumnMeta::new("name", "VARCHAR")]
    }

    fn build(op: Operation) -> Result<BuiltOp> {
        let c = cols();
        let other = |n: &str| if n == "b" { Some("SELECT * FROM \"b\"".to_string()) } else { None };
        op.build(&OpContext { table: "a", current_query: "Q", columns: &c, other_query: &other, query: &no_query, custom: &no_custom })
    }

    #[test]
    fn sort() {
        let b = build(Operation::Sort { column: "name".into(), ascending: false }).unwrap();
        assert_eq!(b.sql, "SELECT * FROM (Q) ORDER BY \"name\" DESC");
        assert_eq!(b.label.key, "step.sort");
        assert_eq!(b.label.args["direction"], "descending");
    }

    #[test]
    fn rename_and_conflicts() {
        let b = build(Operation::Rename { column: "name".into(), new_name: "full name".into() }).unwrap();
        assert_eq!(b.sql, "SELECT \"id\", \"name\" AS \"full name\" FROM (Q)");
        assert!(build(Operation::Rename { column: "name".into(), new_name: "id".into() }).is_err());
        assert!(build(Operation::Rename { column: "nope".into(), new_name: "x".into() }).is_err());
    }

    #[test]
    fn remove_last_column_fails() {
        let c = vec![ColumnMeta::new("only", "BIGINT")];
        let other = |_: &str| None;
        let ctx = OpContext { table: "a", current_query: "Q", columns: &c, other_query: &other, query: &no_query, custom: &no_custom };
        assert!(Operation::Remove { column: "only".into() }.build(&ctx).is_err());
        assert_eq!(build(Operation::Remove { column: "id".into() }).unwrap().sql, "SELECT \"name\" FROM (Q)");
    }

    #[test]
    fn convert_and_conversion_errors() {
        let to = Reading::LocaleNumber { thousands: ".".into(), decimal: ",".into() };
        let conv = "TRY_CAST(replace(replace(\"name\", '.', ''), ',', '.') AS DOUBLE)";
        let b = build(Operation::Convert { column: "name".into(), to: to.clone(), on_error: None, keep_original: true }).unwrap();
        assert_eq!(b.sql, format!("SELECT \"id\", {conv} AS \"name\", \"name\" AS \"name_raw\" FROM (Q)"));
        assert_eq!(b.label.key, "step.convert");
        // A number column is cast to text before a text parse.
        let b = build(Operation::Convert { column: "id".into(), to: Reading::Boolean, on_error: None, keep_original: false }).unwrap();
        assert!(b.sql.contains("lower(CAST(\"id\" AS VARCHAR)) IN ("));
        let b = build(Operation::ConversionErrors { column: "name".into(), to, keep: false }).unwrap();
        assert_eq!(b.sql, format!("SELECT * FROM (Q) WHERE NOT (\"name\" IS NOT NULL AND \"name\" <> '' AND {conv} IS NULL)"));
    }

    #[test]
    fn change_type_validates() {
        let b = build(Operation::ChangeType { column: "id".into(), new_type: "VARCHAR".into() }).unwrap();
        assert_eq!(b.sql, "SELECT TRY_CAST(\"id\" AS VARCHAR) AS \"id\", \"name\" FROM (Q)");
        assert!(build(Operation::ChangeType { column: "id".into(), new_type: "x); drop".into() }).is_err());
    }

    #[test]
    fn drop_null_sentinels() {
        let b = build(Operation::DropNull { column: "id".into(), null_value: None }).unwrap();
        assert_eq!(b.sql, "SELECT * FROM (Q) WHERE \"id\" IS NOT NULL");
        let b = build(Operation::DropNull { column: "id".into(), null_value: Some("-1".into()) }).unwrap();
        assert_eq!(b.sql, "SELECT * FROM (Q) WHERE \"id\" IS NULL OR \"id\" <> -1");
        let b = build(Operation::DropNull { column: "name".into(), null_value: Some("n/a".into()) }).unwrap();
        assert!(b.sql.ends_with("<> 'n/a'"));
    }

    #[test]
    fn filter_values_with_null() {
        let b = build(Operation::FilterValues { column: "name".into(), values: vec![Some("x'y".into()), None] }).unwrap();
        assert_eq!(b.sql, "SELECT * FROM (Q) WHERE CAST(\"name\" AS VARCHAR) IN ('x''y') OR \"name\" IS NULL");
    }

    #[test]
    fn join_and_append_use_current_queries() {
        let b = build(Operation::Join {
            right: "b".into(),
            join_type: JoinType::Left,
            left_key: "id".into(),
            right_key: "id".into(),
        })
        .unwrap();
        assert_eq!(b.sql, "SELECT * FROM (Q) AS \"a\" LEFT JOIN (SELECT * FROM \"b\") AS \"b\" ON \"a\".\"id\" = \"b\".\"id\"");
        assert!(build(Operation::Join { right: "zz".into(), join_type: JoinType::Inner, left_key: "id".into(), right_key: "id".into() }).is_err());
        let b = build(Operation::Append { tables: vec!["b".into()], distinct: false }).unwrap();
        assert_eq!(b.sql, "SELECT * FROM (Q) UNION ALL BY NAME SELECT * FROM (SELECT * FROM \"b\")");
    }

    #[test]
    fn duplicate_picks_free_name() {
        let c = vec![ColumnMeta::new("a", "BIGINT"), ColumnMeta::new("a_copy", "BIGINT")];
        let other = |_: &str| None;
        let ctx = OpContext { table: "t", current_query: "Q", columns: &c, other_query: &other, query: &no_query, custom: &no_custom };
        let b = Operation::Duplicate { column: "a".into() }.build(&ctx).unwrap();
        assert_eq!(b.sql, "SELECT *, \"a\" AS \"a_copy2\" FROM (Q)");
    }

    #[test]
    fn add_column_overwrites_existing() {
        let b = build(Operation::AddColumn { name: "id".into(), expression: "\"id\" * 2".into() }).unwrap();
        assert_eq!(b.sql, "SELECT * REPLACE (\"id\" * 2 AS \"id\") FROM (Q) AS \"a\"");
        let b = build(Operation::AddColumn { name: "x".into(), expression: "1".into() }).unwrap();
        assert_eq!(b.sql, "SELECT *, 1 AS \"x\" FROM (Q) AS \"a\"");
    }

    #[test]
    fn operations_deserialize_from_ui_json() {
        let op: Operation = serde_json::from_str(r#"{"op":"drop_duplicates","keep":"last"}"#).unwrap();
        assert_eq!(op, Operation::DropDuplicates { columns: vec![], keep: Keep::Last });
        let op: Operation = serde_json::from_str(r#"{"op":"join","right":"b","join_type":"full_outer","left_key":"a","right_key":"b"}"#).unwrap();
        assert!(matches!(op, Operation::Join { join_type: JoinType::FullOuter, .. }));
    }
}
