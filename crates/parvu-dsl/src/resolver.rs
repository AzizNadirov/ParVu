//! Resolver + type checker: [`Ast`] -> typed [`Expr`].
//!
//! Binds `table[col]` references against the [`Catalog`], binds function
//! names against the [`FunctionRegistry`], checks arity and infers return
//! types bottom-up.

use crate::DslError;
use crate::catalog::Catalog;
use crate::ir::{Expr, LiteralValue};
use crate::parser::{Ast, BinOp, UnOp};
use crate::registry::{FunctionDef, FunctionRegistry};
use crate::types::LogicalType;

fn err<T>(message: impl Into<String>) -> Result<T, DslError> {
    Err(DslError::Resolve {
        message: message.into(),
    })
}

/// Resolve a parsed AST into a typed IR expression.
pub fn resolve(
    ast: &Ast,
    catalog: &Catalog,
    registry: &FunctionRegistry,
) -> Result<Expr, DslError> {
    Resolver { catalog, registry }.node(ast)
}

struct Resolver<'a> {
    catalog: &'a Catalog,
    registry: &'a FunctionRegistry,
}

/// Strip surrounding double quotes from a bracketed column name. Single
/// quotes are kept, exactly like the Python resolver.
fn column_name(raw: &str) -> String {
    if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
        raw[1..raw.len() - 1].to_string()
    } else {
        raw.to_string()
    }
}

/// Table-level operations produce a full SELECT and cannot be nested inside
/// a scalar expression.
fn ensure_scalar(e: &Expr) -> Result<(), DslError> {
    match e {
        Expr::DropDuplicates { .. } => err(
            "drop_duplicates is a table-level operation and cannot be used inside an expression",
        ),
        Expr::DropNull { .. } => {
            err("drop_null is a table-level operation and cannot be used inside an expression")
        }
        _ => Ok(()),
    }
}

impl Resolver<'_> {
    fn node(&self, ast: &Ast) -> Result<Expr, DslError> {
        match ast {
            Ast::Number(text) => Ok(Expr::Literal {
                value: LiteralValue::from_number_token(text),
                logical_type: LogicalType::Numeric,
            }),
            Ast::String(raw) => {
                let inner = if raw.len() >= 2
                    && ((raw.starts_with('"') && raw.ends_with('"'))
                        || (raw.starts_with('\'') && raw.ends_with('\'')))
                {
                    &raw[1..raw.len() - 1]
                } else {
                    raw.as_str()
                };
                Ok(Expr::Literal {
                    value: LiteralValue::Str(inner.to_string()),
                    logical_type: LogicalType::Text,
                })
            }
            Ast::Boolean(b) => Ok(Expr::boolean(*b)),
            Ast::Null => Ok(Expr::null()),
            Ast::Assignment {
                table,
                column,
                value,
            } => {
                let column = column_name(column);
                if !self.catalog.has_table(table) {
                    return err(format!("Unknown table: '{table}'"));
                }
                let value = self.node(value)?;
                ensure_scalar(&value)?;
                Ok(Expr::Assignment {
                    table: table.clone(),
                    logical_type: value.logical_type(),
                    column,
                    value: Box::new(value),
                })
            }
            Ast::ColumnRef { table, column } => self.column_ref(table, column),
            Ast::FuncCall { name, args } => self.func_call(name, args),
            Ast::MethodCall {
                receiver,
                name,
                args,
            } => self.method_call(receiver, name, args),
            Ast::Binary { op, left, right } => {
                let left = self.node(left)?;
                let right = self.node(right)?;
                ensure_scalar(&left)?;
                ensure_scalar(&right)?;
                let (sym, ty) = match op {
                    BinOp::Add => ("+", LogicalType::Unknown),
                    BinOp::Sub => ("-", LogicalType::Unknown),
                    BinOp::Mul => ("*", LogicalType::Unknown),
                    BinOp::Div => ("/", LogicalType::Unknown),
                    BinOp::Eq => ("==", LogicalType::Boolean),
                    BinOp::Ne => ("!=", LogicalType::Boolean),
                    BinOp::Gt => (">", LogicalType::Boolean),
                    BinOp::Gte => (">=", LogicalType::Boolean),
                    BinOp::Lt => ("<", LogicalType::Boolean),
                    BinOp::Lte => ("<=", LogicalType::Boolean),
                    BinOp::LogicalAnd => ("AND", LogicalType::Boolean),
                    BinOp::LogicalOr => ("OR", LogicalType::Boolean),
                };
                Ok(Expr::BinaryOp {
                    op: sym.to_string(),
                    left: Box::new(left),
                    right: Box::new(right),
                    logical_type: ty,
                })
            }
            Ast::Unary { op, operand } => {
                let operand = self.node(operand)?;
                ensure_scalar(&operand)?;
                Ok(match op {
                    UnOp::Plus => operand,
                    UnOp::Minus => Expr::UnaryOp {
                        op: "-".into(),
                        logical_type: operand.logical_type(),
                        operand: Box::new(operand),
                    },
                    UnOp::Not => Expr::UnaryOp {
                        op: "NOT".into(),
                        operand: Box::new(operand),
                        logical_type: LogicalType::Boolean,
                    },
                })
            }
        }
    }

    fn column_ref(&self, table: &str, raw_column: &str) -> Result<Expr, DslError> {
        let column = column_name(raw_column);
        if !self.catalog.has_table(table) {
            return err(format!("Unknown table: '{table}'"));
        }
        if self.catalog.column_info(table, &column).is_none() {
            return err(format!("Unknown column '{column}' in table '{table}'"));
        }
        Ok(Expr::ColumnRef {
            logical_type: self.catalog.resolve_column(table, &column),
            table: table.to_string(),
            column,
        })
    }

    fn args(&self, args: &[Ast]) -> Result<Vec<Expr>, DslError> {
        args.iter().map(|a| self.node(a)).collect()
    }

    fn func_call(&self, name: &str, args: &[Ast]) -> Result<Expr, DslError> {
        let args = self.args(args)?;
        match name.to_uppercase().as_str() {
            "DROP_DUPLICATES" => return self.drop_duplicates(args),
            "DROP_NULL" => return self.drop_null(args),
            "REPLACE" => return self.replace(args),
            _ => {}
        }
        let Some(func_def) = self.registry.lookup(name) else {
            return err(format!("Unknown function: '{name}'"));
        };
        args.iter().try_for_each(ensure_scalar)?;
        check_args(func_def, args.len())?;
        Ok(Expr::Call {
            func_name: name.to_string(),
            args,
            logical_type: func_def.return_type,
        })
    }

    fn method_call(&self, receiver: &Ast, name: &str, args: &[Ast]) -> Result<Expr, DslError> {
        let receiver = self.node(receiver)?;
        let args = self.args(args)?;
        if name.to_lowercase() == "replace" {
            let mut all = vec![receiver];
            all.extend(args);
            return self.replace(all);
        }
        let Some(func_def) = self.registry.lookup_method(name) else {
            return err(format!(
                "Unknown method '{name}' for type {}",
                receiver.logical_type().name()
            ));
        };
        ensure_scalar(&receiver)?;
        args.iter().try_for_each(ensure_scalar)?;
        check_args(func_def, args.len() + 1)?;
        Ok(Expr::MethodCall {
            receiver: Box::new(receiver),
            method_name: name.to_string(),
            args,
            logical_type: func_def.return_type,
        })
    }

    fn drop_duplicates(&self, args: Vec<Expr>) -> Result<Expr, DslError> {
        let Some(first) = args.first() else {
            return err("drop_duplicates requires at least one column reference");
        };
        let Expr::ColumnRef { table, .. } = first else {
            return err("drop_duplicates arguments must be column references like sales[id]");
        };
        let table = table.clone();
        if !self.catalog.has_table(&table) {
            return err(format!("Unknown table: '{table}'"));
        }
        let mut keep = "first".to_string();
        let mut subset: Vec<String> = Vec::new();
        for arg in &args {
            match arg {
                Expr::ColumnRef {
                    table: t, column, ..
                } => {
                    if *t != table {
                        return err(format!(
                            "drop_duplicates column references must belong to table '{table}'"
                        ));
                    }
                    subset.push(column.clone());
                }
                Expr::Literal {
                    value: LiteralValue::Str(s),
                    ..
                } => {
                    let lower = s.to_lowercase();
                    if lower != "first" && lower != "last" {
                        return err("drop_duplicates keep must be 'first' or 'last'");
                    }
                    keep = lower;
                }
                _ => {
                    return err("drop_duplicates arguments must be column references \
                                or a keep strategy string ('first' or 'last')");
                }
            }
        }
        // A single column reference means "all columns".
        let n_cols = args
            .iter()
            .filter(|a| matches!(a, Expr::ColumnRef { .. }))
            .count();
        if n_cols == 1 {
            subset = self.catalog.columns(&table);
        }
        Ok(Expr::DropDuplicates {
            table,
            columns: subset,
            keep,
            logical_type: LogicalType::Query,
        })
    }

    fn drop_null(&self, args: Vec<Expr>) -> Result<Expr, DslError> {
        let Some(first) = args.first() else {
            return err("drop_null requires a column reference");
        };
        let Expr::ColumnRef { table, column, .. } = first else {
            return err("drop_null first argument must be a column reference like sales[note]");
        };
        if !self.catalog.has_table(table) {
            return err(format!("Unknown table: '{table}'"));
        }
        let mut null_value = LiteralValue::Null;
        if let Some(second) = args.get(1) {
            let Expr::Literal { value, .. } = second else {
                return err(
                    "drop_null second argument must be a literal value (e.g. -1, '', 'N/A')",
                );
            };
            null_value = value.clone();
        }
        if args.len() > 2 {
            return err("drop_null takes at most 2 arguments (column, null_value)");
        }
        Ok(Expr::DropNull {
            table: table.clone(),
            column: column.clone(),
            null_value,
            logical_type: LogicalType::Query,
        })
    }

    fn replace(&self, args: Vec<Expr>) -> Result<Expr, DslError> {
        if args.len() < 3 {
            return err("REPLACE requires at least 3 arguments: text, pattern, with_value");
        }
        args.iter().try_for_each(ensure_scalar)?;
        let flag = |i: usize, what: &str| -> Result<Option<bool>, DslError> {
            match args.get(i) {
                None => Ok(None),
                Some(Expr::Literal {
                    value: LiteralValue::Bool(b),
                    ..
                }) => Ok(Some(*b)),
                Some(_) => err(format!("REPLACE {what} must be TRUE or FALSE")),
            }
        };
        let case_sensitive = flag(3, "case_sensitive")?.unwrap_or(true);
        let regex = flag(4, "regex")?.unwrap_or(false);
        // Extra arguments beyond the fifth are ignored, as in the Python DSL.
        let mut it = args.into_iter();
        let (text, pattern, with_value) =
            (it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
        Ok(Expr::replace(
            text,
            pattern,
            with_value,
            case_sensitive,
            regex,
        ))
    }
}

fn check_args(func_def: &FunctionDef, n_args: usize) -> Result<(), DslError> {
    let required = func_def.params.iter().filter(|p| p.required).count();
    if n_args < required {
        return err(format!(
            "'{}' requires at least {required} arguments, got {n_args}",
            func_def.name
        ));
    }
    // Type checking is permissive (as in the Python DSL): mismatches are allowed.
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Port of tests/unit/core/dsl/test_resolver.py.
    use super::*;
    use crate::{compile, parse};

    fn catalog() -> Catalog {
        let mut c = Catalog::new();
        c.register_table(
            "sales",
            [
                ("revenue", "DOUBLE"),
                ("id", "INTEGER"),
                ("name", "VARCHAR"),
            ]
            .map(|(a, b)| (a.to_string(), b.to_string())),
        );
        c
    }

    fn res(src: &str) -> Result<Expr, DslError> {
        resolve(
            &parse(src).unwrap(),
            &catalog(),
            &FunctionRegistry::default(),
        )
    }

    fn sql(src: &str) -> String {
        compile(&res(src).unwrap(), &FunctionRegistry::default()).unwrap()
    }

    fn err_msg(src: &str) -> String {
        match res(src) {
            Err(DslError::Resolve { message }) => message,
            other => panic!("expected resolve error, got {other:?}"),
        }
    }

    #[test]
    fn resolve_column_ref() {
        match res("sales[revenue]").unwrap() {
            Expr::ColumnRef {
                table,
                column,
                logical_type,
            } => {
                assert_eq!((table.as_str(), column.as_str()), ("sales", "revenue"));
                assert_eq!(logical_type, LogicalType::Numeric);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn resolve_literal_number() {
        assert_eq!(res("42").unwrap(), Expr::int(42));
    }

    #[test]
    fn resolve_function_call() {
        match res("SUM(sales[revenue])").unwrap() {
            Expr::Call {
                func_name,
                logical_type,
                ..
            } => {
                assert_eq!(func_name, "SUM");
                assert_eq!(logical_type, LogicalType::Numeric);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn resolve_binary_op() {
        assert!(
            matches!(res("sales[revenue] + 100").unwrap(), Expr::BinaryOp { op, .. } if op == "+")
        );
    }

    #[test]
    fn resolve_method_call() {
        match res("sales[name].upper").unwrap() {
            Expr::MethodCall {
                method_name,
                logical_type,
                ..
            } => {
                assert_eq!(method_name, "upper");
                assert_eq!(logical_type, LogicalType::Text);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn resolve_unknown_table_raises() {
        assert!(err_msg("unknown[col]").contains("Unknown table"));
    }

    #[test]
    fn resolve_unknown_column_raises() {
        assert!(err_msg("sales[unknown_col]").contains("Unknown column"));
    }

    #[test]
    fn resolve_unknown_function_raises() {
        assert!(err_msg("UNKNOWN_FUNC(sales[revenue])").contains("Unknown function"));
    }

    #[test]
    fn end_to_end_compile() {
        assert_eq!(
            sql("SUM(sales[revenue]) / COUNT(sales[id])"),
            "SUM(sales.revenue) / COUNT(sales.id)"
        );
    }

    #[test]
    fn end_to_end_if() {
        assert_eq!(
            sql("IF(sales[revenue] > 100, 'high', 'low')"),
            "IF(sales.revenue > 100, 'high', 'low')"
        );
    }

    #[test]
    fn resolve_assignment() {
        match res("sales[new_col] = UPPER(sales[name])").unwrap() {
            Expr::Assignment {
                table,
                column,
                logical_type,
                ..
            } => {
                assert_eq!((table.as_str(), column.as_str()), ("sales", "new_col"));
                assert_eq!(logical_type, LogicalType::Text);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn resolve_assignment_unknown_table_raises() {
        assert!(err_msg("unknown[new_col] = 1").contains("Unknown table"));
    }

    #[test]
    fn resolve_assignment_allows_unknown_column() {
        assert!(matches!(
            res("sales[totally_new_column] = sales[revenue] * 2").unwrap(),
            Expr::Assignment { column, .. } if column == "totally_new_column"
        ));
    }

    #[test]
    fn end_to_end_assignment_compile() {
        assert_eq!(
            sql("sales[new_col] = CONCAT(sales[name], '!')"),
            "SELECT *, CONCAT(sales.name, '!') AS \"new_col\" FROM sales"
        );
    }

    #[test]
    fn resolve_drop_duplicates() {
        assert_eq!(
            res("drop_duplicates(sales[id], sales[name], \"first\")").unwrap(),
            Expr::drop_duplicates("sales", vec!["id".into(), "name".into()], "first")
        );
    }

    #[test]
    fn resolve_drop_duplicates_default_all_columns() {
        assert_eq!(
            res("drop_duplicates(sales[id])").unwrap(),
            Expr::drop_duplicates(
                "sales",
                vec!["revenue".into(), "id".into(), "name".into()],
                "first"
            )
        );
    }

    #[test]
    fn resolve_drop_duplicates_single_col_with_keep() {
        assert_eq!(
            res("drop_duplicates(sales[id], \"last\")").unwrap(),
            Expr::drop_duplicates(
                "sales",
                vec!["revenue".into(), "id".into(), "name".into()],
                "last"
            )
        );
    }

    #[test]
    fn resolve_drop_duplicates_last() {
        assert!(matches!(
            res("drop_duplicates(sales[id], sales[name], \"last\")").unwrap(),
            Expr::DropDuplicates { keep, .. } if keep == "last"
        ));
    }

    #[test]
    fn end_to_end_drop_duplicates_compile() {
        let s = sql("drop_duplicates(sales[id], sales[name], \"first\")");
        assert!(s.contains("QUALIFY"));
        assert!(s.contains("ROW_NUMBER()"));
        assert!(s.contains("PARTITION BY id, name"));
    }

    #[test]
    fn resolve_replace_function() {
        match res("REPLACE(sales[name], 'old', 'new')").unwrap() {
            Expr::Replace {
                case_sensitive,
                regex,
                logical_type,
                ..
            } => {
                assert!(case_sensitive);
                assert!(!regex);
                assert_eq!(logical_type, LogicalType::Text);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn resolve_replace_method() {
        match res("sales[name].replace('old', 'new')").unwrap() {
            Expr::Replace {
                text,
                case_sensitive,
                regex,
                ..
            } => {
                assert!(matches!(*text, Expr::ColumnRef { .. }));
                assert!(case_sensitive);
                assert!(!regex);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn resolve_replace_with_flags() {
        assert!(matches!(
            res("REPLACE(sales[name], 'old', 'new', FALSE, TRUE)").unwrap(),
            Expr::Replace {
                case_sensitive: false,
                regex: true,
                ..
            }
        ));
    }

    #[test]
    fn resolve_replace_too_few_args_raises() {
        assert!(err_msg("REPLACE(sales[name], 'old')").contains("at least 3 arguments"));
    }

    #[test]
    fn end_to_end_replace_compile() {
        assert_eq!(
            sql("REPLACE(sales[name], 'old', 'new')"),
            "REPLACE(sales.name, 'old', 'new')"
        );
    }

    #[test]
    fn drop_null_forms() {
        assert_eq!(
            res("drop_null(sales[name])").unwrap(),
            Expr::drop_null("sales", "name", LiteralValue::Null)
        );
        assert_eq!(
            res("drop_null(sales[id], -1)").unwrap(),
            Expr::drop_null("sales", "id", LiteralValue::int(-1))
        );
        assert!(err_msg("drop_null(sales[id], 1, 2)").contains("at most 2"));
        assert!(err_msg("drop_null(1)").contains("column reference"));
    }

    #[test]
    fn unary_operators() {
        assert_eq!(sql("-(sales[revenue] + 1)"), "-(sales.revenue + 1)");
        assert_eq!(sql("NOT sales[id] > 1"), "(NOT sales.id) > 1");
        assert_eq!(sql("NOT (sales[id] > 1)"), "NOT (sales.id > 1)");
        assert_eq!(sql("+sales[id]"), "sales.id");
        assert_eq!(sql("- -5"), "- -5");
    }

    #[test]
    fn nested_table_ops_rejected() {
        assert!(err_msg("UPPER(drop_null(sales[name]))").contains("table-level"));
        assert!(err_msg("sales[x] = drop_duplicates(sales[id])").contains("table-level"));
    }
}
