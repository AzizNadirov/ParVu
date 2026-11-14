//! ParVu expression language (DSL), compiled to DuckDB SQL.
//!
//! Pipeline: [`parse`] (source -> [`Ast`]) -> [`resolve`] (AST -> typed
//! [`ir::Expr`], checked against a [`Catalog`] and a [`FunctionRegistry`]) ->
//! [`compile`] (IR -> SQL). [`translate`] and [`preview`] wrap the whole
//! pipeline the way the query editor uses it.

pub mod catalog;
pub mod compiler;
pub mod ir;
pub mod parser;
pub mod registry;
pub mod resolver;
pub mod types;

use serde::Serialize;

pub use catalog::{Catalog, ColumnInfo};
pub use compiler::compile;
pub use ir::{Expr, LiteralValue};
pub use parser::{Ast, BinOp, UnOp, parse, parse_partial};
pub use registry::{FunctionDef, FunctionRegistry, ParamDef};
pub use resolver::resolve;
pub use types::{LogicalType, duckdb_type_to_logical, methods_for_type};

/// Errors from any stage of the pipeline.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DslError {
    #[error("parse error at line {line}, column {column}: {message}")]
    Parse {
        message: String,
        line: usize,
        column: usize,
    },
    #[error("{message}")]
    Resolve { message: String },
    #[error("{message}")]
    Compile { message: String },
}

/// What an expression does once translated to SQL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TranslationKind {
    /// `table[column] = expr`: the SQL rewrites or appends `column`.
    Assignment { column: String },
    /// `drop_duplicates(...)` / `drop_null(...)`: the SQL is a full query.
    TableQuery,
    /// Any other expression, wrapped as `SELECT <expr> FROM <table>`.
    Scalar,
}

/// Final SQL for an expression plus its classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Translation {
    pub sql: String,
    pub kind: TranslationKind,
}

/// Parse, resolve and compile `src`, returning the IR and compiled SQL.
fn pipeline(
    src: &str,
    catalog: &Catalog,
    registry: &FunctionRegistry,
) -> Result<(Expr, String), DslError> {
    let ast = parse(src.trim())?;
    let expr = resolve(&ast, catalog, registry)?;
    let sql = compile(&expr, registry)?;
    Ok((expr, sql))
}

/// Turn expression text into the SQL the editor executes against `table`
/// (the active table), and classify it.
///
/// * assignments overwrite the column in place (keeping column order) or
///   append it, using the catalog's columns of the assigned table;
/// * table-level operations return their full query;
/// * other expressions become `SELECT <expr> FROM <table>` (just `<expr>`
///   when the catalog is empty).
pub fn translate(
    src: &str,
    catalog: &Catalog,
    registry: &FunctionRegistry,
    table: &str,
) -> Result<Translation, DslError> {
    let (expr, compiled) = pipeline(src, catalog, registry)?;
    match &expr {
        Expr::DropDuplicates { .. } | Expr::DropNull { .. } => Ok(Translation {
            sql: compiled,
            kind: TranslationKind::TableQuery,
        }),
        Expr::Assignment {
            table,
            column,
            value,
            ..
        } => {
            let value_sql = compile(value, registry)?;
            Ok(Translation {
                sql: build_assignment_query(table, column, &value_sql, &catalog.columns(table)),
                kind: TranslationKind::Assignment {
                    column: column.clone(),
                },
            })
        }
        _ => {
            let sql = if catalog.tables().is_empty() {
                compiled
            } else {
                format!("SELECT {compiled} FROM {table}")
            };
            Ok(Translation {
                sql,
                kind: TranslationKind::Scalar,
            })
        }
    }
}

/// `SELECT` that overwrites `column` in place when it exists in `all_cols`,
/// or appends it otherwise. With no known columns, falls back to
/// `SELECT *, <value> AS "<column>"`.
pub fn build_assignment_query(
    table: &str,
    column: &str,
    value_sql: &str,
    all_cols: &[String],
) -> String {
    if all_cols.is_empty() {
        return format!("SELECT *, {value_sql} AS \"{column}\" FROM {table}");
    }
    let mut parts: Vec<String> = all_cols
        .iter()
        .map(|c| {
            if c == column {
                format!("{value_sql} AS \"{c}\"")
            } else {
                format!("\"{c}\"")
            }
        })
        .collect();
    if !all_cols.iter().any(|c| c == column) {
        parts.push(format!("{value_sql} AS \"{column}\""));
    }
    format!("SELECT {} FROM {table}", parts.join(", "))
}

/// SQL preview of an expression (assignments shown as
/// `SELECT *, <expr> AS "<col>" FROM <table>`).
pub fn preview(
    src: &str,
    catalog: &Catalog,
    registry: &FunctionRegistry,
) -> Result<String, DslError> {
    pipeline(src, catalog, registry).map(|(_, sql)| sql)
}

/// SQL for a Replace operation on `table.column`, as built by the Replace
/// dialog. The column reference is still table-qualified; see
/// [`strip_table_qualifier`].
pub fn compile_replace(
    table: &str,
    column: &str,
    pattern: &str,
    replacement: &str,
    case_sensitive: bool,
    regex: bool,
) -> String {
    let expr = Expr::replace(
        Expr::ColumnRef {
            table: table.into(),
            column: column.into(),
            logical_type: LogicalType::Text,
        },
        Expr::string(pattern),
        Expr::string(replacement),
        case_sensitive,
        regex,
    );
    compile(&expr, &FunctionRegistry::empty()).expect("Replace over literals always compiles")
}

/// Remove `table.` qualifiers (Python: `re.sub(rf'\b{re.escape(table)}\.', '', sql)`),
/// used when the SQL is applied to a subquery instead of the named table.
pub fn strip_table_qualifier(sql: &str, table: &str) -> String {
    let pattern = format!(r"\b{}\.", regex::escape(table));
    match regex::Regex::new(&pattern) {
        Ok(re) => re.replace_all(sql, "").into_owned(),
        Err(_) => sql.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        let mut c = Catalog::new();
        c.register_table(
            "sales",
            [("id", "INTEGER"), ("name", "VARCHAR")].map(|(a, b)| (a.to_string(), b.to_string())),
        );
        c
    }

    #[test]
    fn translate_kinds() {
        let r = FunctionRegistry::default();
        let cat = catalog();
        let t = translate("  sales[name] = UPPER(sales[name]) ", &cat, &r, "sales").unwrap();
        assert_eq!(
            t.sql,
            "SELECT \"id\", UPPER(sales.name) AS \"name\" FROM sales"
        );
        assert_eq!(
            t.kind,
            TranslationKind::Assignment {
                column: "name".into()
            }
        );
        let t = translate("sales[x] = 1", &cat, &r, "sales").unwrap();
        assert_eq!(t.sql, "SELECT \"id\", \"name\", 1 AS \"x\" FROM sales");
        let t = translate("drop_null(sales[id])", &cat, &r, "sales").unwrap();
        assert_eq!(t.kind, TranslationKind::TableQuery);
        assert_eq!(t.sql, "SELECT * FROM sales WHERE NOT id IS NULL");
        let t = translate("sales[id] + 1", &cat, &r, "sales").unwrap();
        assert_eq!(t.sql, "SELECT sales.id + 1 FROM sales");
        assert_eq!(t.kind, TranslationKind::Scalar);
        let t = translate("1 + 1", &Catalog::new(), &r, "sales").unwrap();
        assert_eq!(t.sql, "1 + 1");
        assert!(translate("", &cat, &r, "sales").is_err());
    }

    #[test]
    fn new_functions_compile() {
        let r = FunctionRegistry::default();
        let cat = catalog();
        let sql = |src: &str| translate(src, &cat, &r, "sales").unwrap().sql;
        assert_eq!(
            sql("sales[size] = IF(sales[id] > 10, 'big', 'small')"),
            "SELECT \"id\", \"name\", IF(sales.id > 10, 'big', 'small') AS \"size\" FROM sales"
        );
        assert_eq!(sql("GREATEST(sales[id], 0, 5)"), "SELECT GREATEST(sales.id, 0, 5) FROM sales");
        assert_eq!(sql("SUBSTR(sales[name], 1, 3)"), "SELECT SUBSTRING(sales.name, 1, 3) FROM sales");
        assert_eq!(sql("REGEX_MATCH(sales[name], 'a+')"), "SELECT REGEXP_MATCHES(sales.name, 'a+') FROM sales");
        assert_eq!(sql("DATE_TRUNC('month', sales[id])"), "SELECT DATE_TRUNC('month', sales.id) FROM sales");
        assert!(translate("IF(sales[id] > 1)", &cat, &r, "sales").is_err(), "IF needs three arguments");
    }

    #[test]
    fn preview_assignment() {
        let r = FunctionRegistry::default();
        assert_eq!(
            preview("sales[x] = 1", &catalog(), &r).unwrap(),
            "SELECT *, 1 AS \"x\" FROM sales"
        );
    }

    #[test]
    fn replace_helpers() {
        let sql = compile_replace("sales", "name", "a.b", "x", false, false);
        assert_eq!(sql, r"REGEXP_REPLACE(sales.name, 'a\.b', 'x', 'gi')");
        assert_eq!(
            strip_table_qualifier(&sql, "sales"),
            r"REGEXP_REPLACE(name, 'a\.b', 'x', 'gi')"
        );
        assert_eq!(
            strip_table_qualifier("mysales.a + sales.b", "sales"),
            "mysales.a + b"
        );
    }

    #[test]
    fn error_serialization() {
        let e = DslError::Resolve {
            message: "x".into(),
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "resolve");
    }
}
