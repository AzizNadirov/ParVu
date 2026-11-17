//! ParVu expression language (DSL), compiled to DuckDB SQL.
//!
//! Pipeline: [`parse`] (source -> [`Ast`]) -> [`resolve`] (AST -> typed
//! [`ir::Expr`], checked against a [`Catalog`] and a [`FunctionRegistry`]) ->
//! [`compile`] (IR -> SQL). [`translate`] and [`preview`] wrap the whole
//! pipeline the way the query editor uses it.

pub mod catalog;
pub mod compiler;
pub mod excel;
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
pub use resolver::{resolve, resolve_in};
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

/// Aggregate functions: in a formula each is a column total, computed once.
/// Registry functions flagged `aggregate` (custom summary functions) are too.
const AGGREGATES: &[&str] = &["SUM", "AVG", "COUNT", "COUNTD", "MIN", "MAX"];

/// A compiled column formula.
#[derive(Debug, Clone, PartialEq)]
pub struct FormulaSql {
    /// The SQL expression. Column references are qualified with the table;
    /// total `i` appears as [`total_marker`]`(i)`.
    pub sql: String,
    /// The column totals it uses (`SUM([price])`), as aggregate SQL over the
    /// table. ParVu computes them once and writes them in as constants.
    pub totals: Vec<String>,
    /// It reads other rows (`PREV` / `NEXT`): every page then reads the
    /// whole table in order.
    pub reads_neighbours: bool,
}

/// Where total `i` goes in [`FormulaSql::sql`]; `parvu_core::formula`
/// fills it in.
pub fn total_marker(i: usize) -> String {
    format!("\u{1}pv_total_{i}\u{1}")
}

/// Compile a column formula: an expression of one row of `table`, such as
/// `[qty] * [price]` (a leading `=` is allowed, and `[column]` means a
/// column of `table`).
///
/// - `SUM([price])` and the other aggregates are column totals, computed
///   once (see [`FormulaSql::totals`]): `= [price] / SUM([price])`.
/// - `SUM([q1:q4])` adds across the columns q1 to q4 of the row.
/// - `PREV([x])`, `NEXT([x], n)` read neighbouring rows.
pub fn compile_formula(src: &str, catalog: &Catalog, registry: &FunctionRegistry, table: &str) -> Result<FormulaSql, DslError> {
    let text = src.trim();
    let text = text.strip_prefix('=').unwrap_or(text).trim();
    if text.is_empty() {
        return Err(DslError::Resolve { message: "Enter a formula, e.g. = [qty] * [price]".into() });
    }
    let ast = parse(text)?;
    let expr = resolve_in(&ast, catalog, registry, table)?;
    if matches!(expr, Expr::Assignment { .. }) || expr.is_table_level() {
        return Err(DslError::Resolve {
            message: "A formula is an expression of one row, e.g. = [qty] * [price]".into(),
        });
    }
    let mut totals = Vec::new();
    let expr = extract_totals(expr, registry, &mut totals)?;
    let reads_neighbours = reads_neighbours(&expr, registry);
    Ok(FormulaSql { sql: compile(&expr, registry)?, totals, reads_neighbours })
}

fn is_aggregate(name: &str, registry: &FunctionRegistry) -> bool {
    let n = name.to_uppercase();
    AGGREGATES.contains(&n.as_str())
        || excel::AGGREGATES.contains(&n.as_str())
        || registry.lookup(name).is_some_and(|f| f.aggregate)
}

/// Replace each aggregate (a column total) by a marker, collecting its SQL.
fn extract_totals(e: Expr, registry: &FunctionRegistry, out: &mut Vec<String>) -> Result<Expr, DslError> {
    let total = |e: &Expr, out: &mut Vec<String>| -> Result<Expr, DslError> {
        let sql = compile(e, registry)?;
        out.push(sql);
        Ok(Expr::Template { template: total_marker(out.len() - 1), args: Vec::new(), logical_type: e.logical_type(), aggregate: false })
    };
    let each = |v: Vec<Expr>, out: &mut Vec<String>| -> Result<Vec<Expr>, DslError> {
        v.into_iter().map(|a| extract_totals(a, registry, out)).collect()
    };
    Ok(match e {
        Expr::Call { ref func_name, .. } if is_aggregate(func_name, registry) => total(&e, out)?,
        Expr::MethodCall { ref method_name, .. } if is_aggregate(method_name, registry) => total(&e, out)?,
        Expr::Template { aggregate: true, .. } => total(&e, out)?,
        Expr::Call { func_name, args, logical_type } => Expr::Call { func_name, args: each(args, out)?, logical_type },
        Expr::MethodCall { receiver, method_name, args, logical_type } => Expr::MethodCall {
            receiver: Box::new(extract_totals(*receiver, registry, out)?),
            method_name,
            args: each(args, out)?,
            logical_type,
        },
        Expr::BinaryOp { op, left, right, logical_type } => Expr::BinaryOp {
            op,
            left: Box::new(extract_totals(*left, registry, out)?),
            right: Box::new(extract_totals(*right, registry, out)?),
            logical_type,
        },
        Expr::UnaryOp { op, operand, logical_type } => {
            Expr::UnaryOp { op, operand: Box::new(extract_totals(*operand, registry, out)?), logical_type }
        }
        Expr::Replace { text, pattern, with_value, case_sensitive, regex, logical_type } => Expr::Replace {
            text: Box::new(extract_totals(*text, registry, out)?),
            pattern: Box::new(extract_totals(*pattern, registry, out)?),
            with_value: Box::new(extract_totals(*with_value, registry, out)?),
            case_sensitive,
            regex,
            logical_type,
        },
        Expr::Template { template, args, logical_type, aggregate } => {
            Expr::Template { template, args: each(args, out)?, logical_type, aggregate }
        }
        other => other,
    })
}

fn reads_neighbours(e: &Expr, registry: &FunctionRegistry) -> bool {
    let any = |v: &[Expr]| v.iter().any(|a| reads_neighbours(a, registry));
    match e {
        Expr::Template { template, args, .. } => template.contains(" OVER (") || any(args),
        Expr::Call { func_name, args, .. } => registry.lookup(func_name).is_some_and(|f| f.window) || any(args),
        Expr::MethodCall { receiver, args, .. } => reads_neighbours(receiver, registry) || any(args),
        Expr::BinaryOp { left, right, .. } => reads_neighbours(left, registry) || reads_neighbours(right, registry),
        Expr::UnaryOp { operand, .. } => reads_neighbours(operand, registry),
        Expr::Replace { text, pattern, with_value, .. } => {
            reads_neighbours(text, registry) || reads_neighbours(pattern, registry) || reads_neighbours(with_value, registry)
        }
        _ => false,
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
    let col = quote_ident(column);
    if all_cols.is_empty() {
        return format!("SELECT *, {value_sql} AS {col} FROM {table}");
    }
    let mut parts: Vec<String> = all_cols
        .iter()
        .map(|c| {
            if c == column {
                format!("{value_sql} AS {col}")
            } else {
                quote_ident(c)
            }
        })
        .collect();
    if !all_cols.iter().any(|c| c == column) {
        parts.push(format!("{value_sql} AS {col}"));
    }
    format!("SELECT {} FROM {table}", parts.join(", "))
}

/// `"name"` with inner quotes doubled (DuckDB identifier quoting).
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
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

#[cfg(test)]
mod formula_tests {
    use super::*;

    fn catalog() -> Catalog {
        let mut c = Catalog::new();
        c.register_table("sales", [("qty".to_string(), "BIGINT".to_string()), ("unit price".to_string(), "DOUBLE".to_string())]);
        c.register_table("other", [("qty".to_string(), "BIGINT".to_string())]);
        c
    }

    fn quarters() -> Catalog {
        let mut c = Catalog::new();
        c.register_table(
            "t",
            ["id", "q1", "q2", "q3", "q4"].map(|n| (n.to_string(), if n == "id" { "VARCHAR" } else { "DOUBLE" }.to_string())),
        );
        c
    }

    #[test]
    fn formulas() {
        let reg = FunctionRegistry::default();
        let f = |s: &str| compile_formula(s, &catalog(), &reg, "sales");
        assert_eq!(f("= [qty] * [\"unit price\"]").unwrap(), f("sales[qty] * sales[\"unit price\"]").unwrap());
        assert!(f("= ROUND([qty] / 3, 1)").unwrap().sql.contains("ROUND"));
        assert!(f("= [nope] + 1").is_err());
        assert!(f("sales[x] = 1").is_err(), "assignment");
        assert!(f("= ").is_err());
        // A bare reference outside formulas: the only table, else an error.
        let mut one = Catalog::new();
        one.register_table("t", [("a".to_string(), "BIGINT".to_string())]);
        assert!(resolve(&parse("[a] + 1").unwrap(), &one, &reg).is_ok());
        assert!(resolve(&parse("[qty] + 1").unwrap(), &catalog(), &reg).is_err());
    }

    #[test]
    fn totals_become_markers() {
        let reg = FunctionRegistry::default();
        let f = compile_formula("= [qty] / SUM([qty]) + MAX([qty])", &catalog(), &reg, "sales").unwrap();
        assert_eq!(f.totals, ["SUM(sales.qty)", "MAX(sales.qty)"]);
        assert_eq!(f.sql, format!("sales.qty / ({}) + ({})", total_marker(0), total_marker(1)));
        assert!(!f.reads_neighbours);
        // A total inside a function, and a plain formula has none.
        let g = compile_formula("= ROUND(100 * [qty] / SUM([qty]), 1)", &catalog(), &reg, "sales").unwrap();
        assert_eq!(g.totals, ["SUM(sales.qty)"]);
        assert!(compile_formula("= [qty] + 1", &catalog(), &reg, "sales").unwrap().totals.is_empty());
    }

    #[test]
    fn ranges_add_across_a_row() {
        let reg = FunctionRegistry::default();
        let f = |s: &str| compile_formula(s, &quarters(), &reg, "t").map(|f| f.sql);
        assert_eq!(f("= SUM([q1:q3])").unwrap(), "(COALESCE(t.q1, 0) + COALESCE(t.q2, 0) + COALESCE(t.q3, 0))");
        // Either direction, and quoted names.
        assert_eq!(f("= SUM([q3:q1])").unwrap(), f("= SUM([\"q1\":\"q3\"])").unwrap());
        assert_eq!(f("= MAX([q1:q2])").unwrap(), "(GREATEST(t.q1, t.q2))");
        assert_eq!(f("= MIN([q1:q2])").unwrap(), "(LEAST(t.q1, t.q2))");
        assert_eq!(
            f("= COUNT([q1:q2])").unwrap(),
            "(CAST(t.q1 IS NOT NULL AS INTEGER) + CAST(t.q2 IS NOT NULL AS INTEGER))"
        );
        assert!(f("= AVG([q1:q4])").unwrap().contains("NULLIF("));
        // A range is a row-wise total; it is not a column total.
        let c = compile_formula("= SUM([q1:q4]) / SUM([q1])", &quarters(), &reg, "t").unwrap();
        assert_eq!(c.totals, ["SUM(t.q1)"]);
        assert!(f("= [q1:q2] + 1").unwrap_err().to_string().contains("SUM"));
        assert!(f("= ROUND([q1:q2])").is_err());
        assert!(f("= SUM([q1:nope])").is_err());
    }

    #[test]
    fn parse_errors_in_words() {
        let reg = FunctionRegistry::default();
        let e = compile_formula("= [qty] + * 2", &catalog(), &reg, "sales").unwrap_err();
        match e {
            DslError::Parse { message, line, column } => {
                assert_eq!(message, "Expected a value, a column like [qty] or a function here");
                assert_eq!((line, column), (1, 9));
            }
            other => panic!("{other:?}"),
        }
        let e = compile_formula("[qty] 2", &catalog(), &reg, "sales").unwrap_err().to_string();
        assert!(e.contains("an operator such as + or *"), "{e}");
        assert!(e.contains("the end of the expression"), "{e}");
    }

    /// The registry with two custom functions, as the app builds them.
    fn with_custom() -> FunctionRegistry {
        let mut reg = FunctionRegistry::default();
        let mut phone = ParamDef::new("phone", LogicalType::Text);
        phone.accepts = Some(vec![LogicalType::Text]);
        reg.register(FunctionDef::custom(
            "clean_phone",
            vec![phone, ParamDef::with_default("prefix", LogicalType::Text, "CAST('+' AS VARCHAR)")],
            LogicalType::Text,
            "Digits only",
            "regexp_replace(__pv_arg_0, '[0-9]{2}', '', 'g') || CAST(__pv_arg_1 AS VARCHAR)",
        ));
        let num = || {
            let mut p = ParamDef::new("x", LogicalType::Numeric);
            p.accepts = Some(vec![LogicalType::Numeric]);
            p
        };
        let mut w = num();
        w.name = "w".into();
        let mut avg = FunctionDef::custom("weighted_average", vec![num(), w], LogicalType::Numeric, "", "(sum((__pv_arg_0 * __pv_arg_1)) / sum(__pv_arg_1))");
        avg.aggregate = true;
        reg.register(avg);
        let mut run = FunctionDef::custom("running", vec![num()], LogicalType::Numeric, "", "sum(__pv_arg_0) OVER (ROWS UNBOUNDED PRECEDING)");
        run.window = true;
        reg.register(run);
        reg
    }

    #[test]
    fn custom_functions_in_formulas() {
        let reg = with_custom();
        let mut cat = catalog();
        cat.register_table("sales", [("qty", "BIGINT"), ("phone", "VARCHAR")].map(|(a, b)| (a.to_string(), b.to_string())));
        let f = |s: &str| compile_formula(s, &cat, &reg, "sales");
        // Slots are filled, `{2}` in the body is left alone, the default fills the optional one.
        assert_eq!(
            f("= clean_phone([phone])").unwrap().sql,
            "(regexp_replace((sales.phone), '[0-9]{2}', '', 'g') || CAST(CAST('+' AS VARCHAR) AS VARCHAR))"
        );
        assert!(f("= clean_phone([phone], \"00\")").unwrap().sql.contains("CAST(('00') AS VARCHAR)"));
        // A summary function is a column total, like SUM.
        let t = f("= [qty] - weighted_average([qty], [qty])").unwrap();
        assert_eq!(t.totals, ["((sum(((sales.qty) * (sales.qty))) / sum((sales.qty))))"]);
        assert_eq!(t.sql, format!("sales.qty - ({})", total_marker(0)));
        // A window reads other rows.
        assert!(f("= running([qty])").unwrap().reads_neighbours);
        assert!(!f("= clean_phone([phone])").unwrap().reads_neighbours);
        // Typed parameters and a fixed number of arguments.
        let e = f("= clean_phone([qty])").unwrap_err().to_string();
        assert_eq!(e, "clean_phone: phone takes text, but this argument is a number");
        assert!(f("= weighted_average([phone], [qty])").unwrap_err().to_string().contains("takes a number"));
        assert!(f("= clean_phone([phone], 'a', 'b')").unwrap_err().to_string().contains("at most 2"));
        assert!(f("= clean_phone()").is_err());
        // An expression of unknown type is let through (DuckDB checks it).
        assert!(f("= clean_phone([qty] + 1)").is_ok());
    }

    #[test]
    fn slots_fill_exactly() {
        use crate::compiler::fill_slots;
        let args = vec!["a".to_string(), "b".to_string()];
        assert_eq!(fill_slots("__pv_arg_0 + __pv_arg_1 + {0}", &args), "a + b + {0}");
        assert_eq!(fill_slots("__pv_arg_12 __pv_arg_", &args), "__pv_arg_12 __pv_arg_");
        // An argument containing a slot is not filled again.
        assert_eq!(fill_slots("__pv_arg_0 __pv_arg_1", &["__pv_arg_1".to_string(), "x".to_string()]), "__pv_arg_1 x");
    }

    #[test]
    fn prev_and_next() {
        let reg = FunctionRegistry::default();
        let f = |s: &str| compile_formula(s, &quarters(), &reg, "t");
        let p = f("= [q1] - PREV([q1])").unwrap();
        assert_eq!(p.sql, "t.q1 - (LAG(t.q1, 1) OVER ())");
        assert!(p.reads_neighbours);
        assert_eq!(f("= NEXT([q2], 3)").unwrap().sql, "(LEAD(t.q2, 3) OVER ())");
        assert!(f("= PREV([q1], 0)").is_err());
        assert!(f("= PREV([q1], [q2])").is_err());
        assert!(f("= PREV()").is_err());
    }
}
