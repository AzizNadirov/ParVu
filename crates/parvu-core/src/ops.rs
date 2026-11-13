//! Table operations: one definition per operation.
//!
//! Each [`Operation`] builds the SQL that wraps the tab's current query, plus
//! a translatable [`StepLabel`] for the applied-steps panel. The UI's menus,
//! dialogs and column context menu all send these, and the planned AI tools
//! will use the same enum.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::page::{Cell, ColumnMeta};
use crate::sql::{is_safe_type_name, quote_ident, quote_literal};

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
#[serde(rename_all = "lowercase")]
pub enum Keep {
    #[default]
    First,
    Last,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Operation {
    Sort { column: String, ascending: bool },
    Rename { column: String, new_name: String },
    Remove { column: String },
    ChangeType { column: String, new_type: String },
    Duplicate { column: String },
    DropDuplicates {
        /// Empty means all columns.
        #[serde(default)]
        columns: Vec<String>,
        #[serde(default)]
        keep: Keep,
    },
    DropNull {
        column: String,
        /// `None` drops SQL NULLs; otherwise drops rows equal to this sentinel.
        #[serde(default)]
        null_value: Option<String>,
    },
    /// Keep rows whose value (as text) is one of `values`; `None` matches NULL.
    FilterValues { column: String, values: Vec<Cell> },
    Join { right: String, join_type: JoinType, left_key: String, right_key: String },
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
    },
}

/// What an operation needs to know about the tab it runs on.
pub struct OpContext<'a> {
    pub table: &'a str,
    pub current_query: &'a str,
    pub columns: &'a [ColumnMeta],
    /// Current query of another tab by name, for joins and appends.
    pub other_query: &'a dyn Fn(&str) -> Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltOp {
    pub sql: String,
    pub label: StepLabel,
}

impl<'a> OpContext<'a> {
    fn require(&self, column: &str) -> Result<()> {
        if self.columns.iter().any(|c| c.name == column) {
            Ok(())
        } else {
            Err(CoreError::UnknownColumn(column.to_string()))
        }
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
            Operation::ChangeType { column, new_type } => {
                ctx.require(column)?;
                if !is_safe_type_name(new_type) {
                    return Err(CoreError::invalid(format!("Invalid type: {new_type}")));
                }
                (
                    ctx.select_each(|c| {
                        if c == column { format!("CAST({} AS {new_type}) AS {}", q(c), q(c)) } else { q(c) }
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
                let subset: Vec<String> = if columns.is_empty() {
                    ctx.columns.iter().map(|c| c.name.clone()).collect()
                } else {
                    for c in columns {
                        ctx.require(c)?;
                    }
                    columns.clone()
                };
                if subset.is_empty() {
                    return Err(CoreError::invalid("No columns"));
                }
                let partition = subset.iter().map(|c| q(c)).collect::<Vec<_>>().join(", ");
                let order = match keep {
                    Keep::First => "ASC",
                    Keep::Last => "DESC",
                };
                let keep_word = match keep {
                    Keep::First => "first",
                    Keep::Last => "last",
                };
                (
                    format!(
                        "SELECT * EXCLUDE (__pv_rn) FROM (SELECT *, ROW_NUMBER() OVER () AS __pv_rn FROM ({cur})) \
                         QUALIFY ROW_NUMBER() OVER (PARTITION BY {partition} ORDER BY __pv_rn {order}) = 1 ORDER BY __pv_rn"
                    ),
                    StepLabel::new("step.drop_duplicates", &[("keep", keep_word), ("columns", &subset.join(", "))]),
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
                let keyword = if *distinct { "UNION" } else { "UNION ALL" };
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
            Operation::Replace { column, pattern, output_column, expression } => {
                ctx.require(column)?;
                if output_column.trim().is_empty() {
                    return Err(CoreError::invalid("Output column name is empty"));
                }
                let t = q(ctx.table);
                let sql = if ctx.columns.iter().any(|c| &c.name == output_column) {
                    format!("SELECT * REPLACE ({expression} AS {}) FROM ({cur}) AS {t}", q(output_column))
                } else {
                    format!("SELECT *, {expression} AS {} FROM ({cur}) AS {t}", q(output_column))
                };
                (sql, StepLabel::new("step.replace", &[("column", column), ("pattern", pattern)]))
            }
        };
        Ok(BuiltOp { sql, label })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols() -> Vec<ColumnMeta> {
        vec![ColumnMeta::new("id", "BIGINT"), ColumnMeta::new("name", "VARCHAR")]
    }

    fn build(op: Operation) -> Result<BuiltOp> {
        let c = cols();
        let other = |n: &str| if n == "b" { Some("SELECT * FROM \"b\"".to_string()) } else { None };
        op.build(&OpContext { table: "a", current_query: "Q", columns: &c, other_query: &other })
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
        let ctx = OpContext { table: "a", current_query: "Q", columns: &c, other_query: &other };
        assert!(Operation::Remove { column: "only".into() }.build(&ctx).is_err());
        assert_eq!(build(Operation::Remove { column: "id".into() }).unwrap().sql, "SELECT \"name\" FROM (Q)");
    }

    #[test]
    fn change_type_validates() {
        let b = build(Operation::ChangeType { column: "id".into(), new_type: "VARCHAR".into() }).unwrap();
        assert_eq!(b.sql, "SELECT CAST(\"id\" AS VARCHAR) AS \"id\", \"name\" FROM (Q)");
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
        assert_eq!(b.sql, "SELECT * FROM (Q) UNION ALL SELECT * FROM (SELECT * FROM \"b\")");
    }

    #[test]
    fn duplicate_picks_free_name() {
        let c = vec![ColumnMeta::new("a", "BIGINT"), ColumnMeta::new("a_copy", "BIGINT")];
        let other = |_: &str| None;
        let ctx = OpContext { table: "t", current_query: "Q", columns: &c, other_query: &other };
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
