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
    Resolver { catalog, registry, current: None }.node(ast)
}

struct Resolver<'a> {
    catalog: &'a Catalog,
    registry: &'a FunctionRegistry,
    /// The table of a bare `[column]` reference.
    current: Option<&'a str>,
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
/// Like [`resolve`], with bare `[column]` references taken from `table`.
pub fn resolve_in(ast: &Ast, catalog: &Catalog, registry: &FunctionRegistry, table: &str) -> Result<Expr, DslError> {
    Resolver { catalog, registry, current: Some(table) }.node(ast)
}

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
            Ast::RangeRef { .. } => err("A range [a:c] works inside SUM, AVG, MIN, MAX or COUNT, e.g. SUM([q1:q4])"),
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

    /// The table a bare `[column]` means: the current one, or the only one open.
    fn bare_table(&self, column: &str) -> Result<String, DslError> {
        let only = self.catalog.tables();
        match self.current {
            Some(t) => Ok(t.to_string()),
            None if only.len() == 1 => Ok(only[0].clone()),
            None => err(format!("Write the table name: table[{column}]")),
        }
    }

    fn column_ref(&self, table: &str, raw_column: &str) -> Result<Expr, DslError> {
        let column = column_name(raw_column);
        let bare;
        let table = if table.is_empty() {
            bare = self.bare_table(&column)?;
            bare.as_str()
        } else {
            table
        };
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
        // Excel's FLOOR(number, significance); FLOOR(number) stays ParVu's.
        let name = if name.eq_ignore_ascii_case("FLOOR") && args.len() == 2 { "FLOOR.MATH" } else { name };
        let upper = name.to_uppercase();
        if let [Ast::RangeRef { table, from, to }] = args
            && ROW_FUNCS.contains(&upper.as_str())
        {
            return self.row_range(&upper, table, from, to);
        }
        if upper == "PREV" || upper == "NEXT" {
            return self.neighbour(&upper, args);
        }
        if crate::excel::VARIADIC.contains(&upper.as_str()) {
            return self.variadic(&upper, args);
        }
        if crate::excel::CONDITIONAL.contains(&upper.as_str()) {
            return self.conditional(&upper, args);
        }
        match upper.as_str() {
            "XLOOKUP" | "VLOOKUP" => return self.lookup(&upper, args),
            "TEXT" => return self.text(args),
            _ => {}
        }
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
        check_types(func_def, &args)?;
        Ok(Expr::Call {
            func_name: name.to_string(),
            args,
            logical_type: func_def.return_type,
        })
    }

    /// Arguments of a function that takes any number of them; a range
    /// `[a:c]` counts as its columns.
    fn spread_args(&self, args: &[Ast]) -> Result<Vec<Expr>, DslError> {
        let mut out = Vec::new();
        for a in args {
            match a {
                Ast::RangeRef { table, from, to } => out.extend(self.range_columns(table, from, to)?),
                other => out.push(self.node(other)?),
            }
        }
        out.iter().try_for_each(ensure_scalar)?;
        Ok(out)
    }

    /// The columns of `[from:to]` (or `table[from:to]`), in table order.
    fn range_columns(&self, table: &str, from: &str, to: &str) -> Result<Vec<Expr>, DslError> {
        let (from, to) = (column_name(from), column_name(to));
        let table = if table.is_empty() { self.bare_table(&from)? } else { table.to_string() };
        if !self.catalog.has_table(&table) {
            return err(format!("Unknown table: '{table}'"));
        }
        let columns = self.catalog.columns(&table);
        let position = |c: &str| {
            columns.iter().position(|x| x == c).ok_or_else(|| DslError::Resolve {
                message: format!("Unknown column '{c}' in table '{table}'"),
            })
        };
        let (a, b) = (position(&from)?, position(&to)?);
        Ok(columns[a.min(b)..=a.max(b)]
            .iter()
            .map(|c| Expr::ColumnRef {
                logical_type: self.catalog.resolve_column(&table, c),
                table: table.clone(),
                column: c.clone(),
            })
            .collect())
    }

    /// Excel's functions with any number of arguments.
    fn variadic(&self, func: &str, args: &[Ast]) -> Result<Expr, DslError> {
        let args = self.spread_args(args)?;
        let n = args.len();
        let each = |range: std::ops::Range<usize>, f: &dyn Fn(usize) -> String, sep: &str| {
            range.map(f).collect::<Vec<_>>().join(sep)
        };
        let need = |min: usize, usage: &str| -> Result<(), DslError> {
            if n < min { err(format!("{func} needs {usage}")) } else { Ok(()) }
        };
        let (template, ty) = match func {
            "AND" | "OR" => {
                need(1, "at least one condition")?;
                (each(0..n, &|i| format!("({{{i}}})"), if func == "AND" { " AND " } else { " OR " }), LogicalType::Boolean)
            }
            "XOR" => {
                need(1, "at least one condition")?;
                (format!("({}) % 2 = 1", each(0..n, &|i| format!("CAST(({{{i}}}) AS INTEGER)"), " + ")), LogicalType::Boolean)
            }
            "SUMPRODUCT" => {
                need(2, "at least two columns")?;
                let t = format!("sum({})", each(0..n, &|i| format!("({{{i}}})"), " * "));
                return Ok(Expr::Template { template: t, args, logical_type: LogicalType::Numeric, aggregate: true });
            }
            "PRODUCT" => {
                need(1, "at least one number")?;
                (each(0..n, &|i| format!("COALESCE(({{{i}}}), 1)"), " * "), LogicalType::Numeric)
            }
            "CONCATENATE" => {
                need(1, "at least one text")?;
                (format!("concat({})", each(0..n, &|i| format!("({{{i}}})"), ", ")), LogicalType::Text)
            }
            "TEXTJOIN" => {
                need(3, "a delimiter, TRUE or FALSE (skip empty values), and texts")?;
                let texts = |skip: bool| {
                    each(2..n, &|i| if skip { format!("NULLIF(CAST(({{{i}}}) AS VARCHAR), '')") } else { format!("COALESCE(CAST(({{{i}}}) AS VARCHAR), '')") }, ", ")
                };
                let t = match &args[1] {
                    Expr::Literal { value: LiteralValue::Bool(b), .. } => format!("concat_ws(({{0}}), {})", texts(*b)),
                    _ => format!("CASE WHEN ({{1}}) THEN concat_ws(({{0}}), {}) ELSE concat_ws(({{0}}), {}) END", texts(true), texts(false)),
                };
                (t, LogicalType::Text)
            }
            "IFS" => {
                if n < 2 || n % 2 != 0 {
                    return err("IFS takes pairs of condition and value, e.g. IFS([x] > 10, 'high', TRUE, 'low')");
                }
                let whens = each(0..n / 2, &|k| format!("WHEN ({{{}}}) THEN ({{{}}})", 2 * k, 2 * k + 1), " ");
                (format!("CASE {whens} END"), args[1].logical_type())
            }
            "SWITCH" => {
                if n < 3 {
                    return err("SWITCH takes an expression, then pairs of value and result, and an optional default");
                }
                let pairs = (n - 1) / 2;
                let whens = each(0..pairs, &|k| format!("WHEN ({{{}}}) THEN ({{{}}})", 1 + 2 * k, 2 + 2 * k), " ");
                let default = if (n - 1) % 2 == 1 { format!(" ELSE ({{{}}})", n - 1) } else { String::new() };
                (format!("CASE ({{0}}) {whens}{default} END"), args[2].logical_type())
            }
            _ => {
                // CHOOSE
                need(2, "an index and at least one value")?;
                let whens = each(1..n, &|i| format!("WHEN {i} THEN ({{{i}}})"), " ");
                (format!("CASE CAST(({{0}}) AS BIGINT) {whens} END"), args[1].logical_type())
            }
        };
        Ok(Expr::Template { template, args, logical_type: ty, aggregate: false })
    }

    /// COUNTIF and friends. Criteria that are values ('North', '>10') make a
    /// column total; a criterion that is the range's own column
    /// (COUNTIF([region], [region])) compares with this row: a window.
    fn conditional(&self, func: &str, args: &[Ast]) -> Result<Expr, DslError> {
        let args = self.args(args)?;
        args.iter().try_for_each(ensure_scalar)?;
        let n = args.len();
        let usage = |u: &str| err(format!("{func} takes {u}"));
        // (index of the values to total, first criteria pair)
        let (value, first) = match func {
            "COUNTIF" if n == 2 => (None, 0),
            "COUNTIF" => return usage("a range and a criteria, e.g. COUNTIF([region], 'North')"),
            "COUNTIFS" if n >= 2 && n % 2 == 0 => (None, 0),
            "COUNTIFS" => return usage("pairs of range and criteria"),
            "SUMIF" | "AVERAGEIF" if n == 2 || n == 3 => (Some(if n == 3 { 2 } else { 0 }), 0),
            "SUMIF" | "AVERAGEIF" => return usage("a range, a criteria and optionally the values to add"),
            _ if n >= 3 && n % 2 == 1 => (Some(0), 1),
            _ => return usage("the values, then pairs of range and criteria"),
        };
        let mut conds = Vec::new();
        let mut parts = Vec::new();
        // SUMIF / AVERAGEIF: one pair, then the values to total.
        let end = if func == "SUMIF" || func == "AVERAGEIF" { 2 } else { n };
        for k in (first..end).step_by(2) {
            let (r, c) = (k, k + 1);
            match &args[c] {
                Expr::Literal { value, .. } => conds.push(crate::excel::criterion(r, value)),
                e if *e == args[r] => parts.push(format!("({{{r}}})")),
                _ => {
                    return err(format!(
                        "{func}'s criteria is a value ('North', '>10') or the range's own column for this row, e.g. COUNTIF([region], [region])"
                    ));
                }
            }
        }
        let agg = match (func, value) {
            (f, _) if f.starts_with("COUNT") => "count(*)".to_string(),
            (f, Some(v)) if f.starts_with("SUM") => format!("sum(({{{v}}}))"),
            (f, Some(v)) if f.starts_with("AVERAGE") => format!("avg(({{{v}}}))"),
            ("MAXIFS", Some(v)) => format!("max(({{{v}}}))"),
            (_, Some(v)) => format!("min(({{{v}}}))"),
            _ => unreachable!(),
        };
        let filter = if conds.is_empty() { String::new() } else { format!(" FILTER (WHERE {})", conds.join(" AND ")) };
        let (template, aggregate) = if parts.is_empty() {
            (format!("{agg}{filter}"), true)
        } else {
            (format!("{agg}{filter} OVER (PARTITION BY {})", parts.join(", ")), false)
        };
        Ok(Expr::Template { template, args, logical_type: LogicalType::Numeric, aggregate })
    }

    /// XLOOKUP(value, other[key], other[result], [if_not_found]) and
    /// VLOOKUP(value, other[key:…], column, [FALSE]): the value of the first
    /// matching row of another table.
    fn lookup(&self, func: &str, args: &[Ast]) -> Result<Expr, DslError> {
        let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let (value, table, key, result, fallback) = if func == "XLOOKUP" {
            if !(3..=4).contains(&args.len()) {
                return err("XLOOKUP takes a value, the column to search and the column to return, e.g. XLOOKUP([id], prices[id], prices[price])");
            }
            let key = self.node(&args[1])?;
            let result = self.node(&args[2])?;
            let (Expr::ColumnRef { table: t1, column: k, .. }, Expr::ColumnRef { table: t2, column: r, .. }) = (&key, &result) else {
                return err("XLOOKUP searches and returns columns of another table, e.g. prices[id] and prices[price]");
            };
            if t1 != t2 {
                return err("XLOOKUP's search and return columns must come from the same table");
            }
            (self.node(&args[0])?, t1.clone(), k.clone(), r.clone(), args.get(3).map(|a| self.node(a)).transpose()?)
        } else {
            if !(3..=4).contains(&args.len()) {
                return err("VLOOKUP takes a value, a range of another table and a column number, e.g. VLOOKUP([id], prices[id:price], 2)");
            }
            let Ast::RangeRef { table, from, to } = &args[1] else {
                return err("VLOOKUP's second argument is a range of another table, e.g. prices[id:price]");
            };
            let cols = self.range_columns(table, from, to)?;
            let Expr::Literal { value: LiteralValue::Int(n), .. } = self.node(&args[2])? else {
                return err("VLOOKUP's column number is a whole number, e.g. 2");
            };
            let n: usize = n.parse().unwrap_or(0);
            if n == 0 || n > cols.len() {
                return err(format!("VLOOKUP's range has {} columns; the column number must be 1 to {}", cols.len(), cols.len()));
            }
            if let Some(a) = args.get(3)
                && !matches!(self.node(a)?, Expr::Literal { value: LiteralValue::Bool(false), .. })
            {
                return err("VLOOKUP finds exact matches only: write FALSE as the last argument, or leave it out");
            }
            let column = |e: &Expr| match e {
                Expr::ColumnRef { table, column, .. } => (table.clone(), column.clone()),
                _ => unreachable!(),
            };
            let (t, k) = column(&cols[0]);
            let (_, r) = column(&cols[n - 1]);
            (self.node(&args[0])?, t, k, r, None)
        };
        if self.current == Some(table.as_str()) {
            return err(format!("{func} looks up values in another open table"));
        }
        ensure_scalar(&value)?;
        let found = format!(
            "SELECT __pv_lookup.{} FROM {} AS __pv_lookup WHERE __pv_lookup.{} = ({{0}}) LIMIT 1",
            quote(&result),
            quote(&table),
            quote(&key)
        );
        let (template, args) = match fallback {
            Some(f) => (format!("COALESCE(({found}), ({{1}}))"), vec![value, f]),
            None => (format!("({found})"), vec![value]),
        };
        Ok(Expr::Template { template, args, logical_type: LogicalType::Unknown, aggregate: false })
    }

    /// TEXT(value, 'format'): Excel's format codes.
    fn text(&self, args: &[Ast]) -> Result<Expr, DslError> {
        let args = self.args(args)?;
        let [value, Expr::Literal { value: LiteralValue::Str(format), .. }] = args.as_slice() else {
            return err("TEXT takes a value and an Excel format in quotes, e.g. TEXT([date], 'dd.mm.yyyy')");
        };
        ensure_scalar(value)?;
        let template = crate::excel::text_format(format).map_err(|m| DslError::Resolve { message: m })?;
        Ok(Expr::Template { template, args: vec![value.clone()], logical_type: LogicalType::Text, aggregate: false })
    }

    /// `SUM([q1:q4])` and friends: across the columns of one row, NULLs
    /// skipped (a single column inside SUM stays the column's total).
    fn row_range(&self, func: &str, table: &str, from: &str, to: &str) -> Result<Expr, DslError> {
        let args = self.range_columns(table, from, to)?;
        let each = |f: &dyn Fn(usize) -> String, sep: &str| (0..args.len()).map(f).collect::<Vec<_>>().join(sep);
        let sum = || each(&|i| format!("COALESCE({{{i}}}, 0)"), " + ");
        let count = || each(&|i| format!("CAST({{{i}}} IS NOT NULL AS INTEGER)"), " + ");
        let (template, ty) = match func {
            "SUM" => (sum(), LogicalType::Numeric),
            "COUNT" => (count(), LogicalType::Numeric),
            "AVG" => (format!("({}) / NULLIF({}, 0)", sum(), count()), LogicalType::Numeric),
            "MIN" => (format!("LEAST({})", each(&|i| format!("{{{i}}}"), ", ")), args[0].logical_type()),
            _ => (format!("GREATEST({})", each(&|i| format!("{{{i}}}"), ", ")), args[0].logical_type()),
        };
        Ok(Expr::Template { template, args, logical_type: ty, aggregate: false })
    }

    /// `PREV([x])`, `NEXT([x], 2)`: the value `n` rows before or after, in
    /// the table's row order (NULL past the ends).
    fn neighbour(&self, func: &str, args: &[Ast]) -> Result<Expr, DslError> {
        let usage = || err(format!("{func} takes a column and an optional number of rows, e.g. {func}([total]) or {func}([total], 2)"));
        let mut args = self.args(args)?;
        if args.is_empty() || args.len() > 2 {
            return usage();
        }
        args.iter().try_for_each(ensure_scalar)?;
        let n = match args.get(1) {
            None => 1,
            Some(Expr::Literal { value: LiteralValue::Int(v), .. }) => match v.parse::<u32>() {
                Ok(n) if n >= 1 => n,
                _ => return usage(),
            },
            Some(_) => return usage(),
        };
        args.truncate(1);
        let window = if func == "PREV" { "LAG" } else { "LEAD" };
        Ok(Expr::Template {
            template: format!("{window}({{0}}, {n}) OVER ()"),
            logical_type: args[0].logical_type(),
            args,
            aggregate: false,
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

/// Functions that take a range of columns, `SUM([q1:q4])`.
const ROW_FUNCS: &[&str] = &["SUM", "AVG", "MIN", "MAX", "COUNT"];

fn check_args(func_def: &FunctionDef, n_args: usize) -> Result<(), DslError> {
    let required = func_def.params.iter().filter(|p| p.required).count();
    if n_args < required {
        return err(format!(
            "'{}' requires at least {required} arguments, got {n_args}",
            func_def.name
        ));
    }
    // Custom functions have a fixed signature: no extra arguments.
    if func_def.custom && n_args > func_def.params.len() {
        let n = func_def.params.len();
        return err(format!("'{}' takes at most {n} argument{}, got {n_args}", func_def.name, if n == 1 { "" } else { "s" }));
    }
    Ok(())
}

/// A word for a logical type in messages.
fn type_word(t: LogicalType) -> &'static str {
    match t {
        LogicalType::Numeric => "a number",
        LogicalType::Text => "text",
        LogicalType::Boolean => "yes/no",
        LogicalType::Date => "a date",
        LogicalType::Timestamp => "a date and time",
        LogicalType::List => "a list",
        LogicalType::Struct => "a struct",
        LogicalType::Unknown | LogicalType::Query => "a value",
    }
}

/// Type checking is permissive for the built-ins (as in the Python DSL).
/// A parameter that lists the types it `accepts` (custom functions) refuses
/// an argument whose type is known and not among them.
fn check_types(func_def: &FunctionDef, args: &[Expr]) -> Result<(), DslError> {
    for (p, a) in func_def.params.iter().zip(args) {
        let Some(accepts) = &p.accepts else { continue };
        let got = a.logical_type();
        if matches!(got, LogicalType::Unknown | LogicalType::Query) || accepts.is_empty() || accepts.contains(&got) {
            continue;
        }
        let wanted: Vec<&str> = accepts.iter().map(|t| type_word(*t)).collect();
        return err(format!(
            "{}: {} takes {}, but this argument is {}",
            func_def.name,
            p.name,
            wanted.join(" or "),
            type_word(got)
        ));
    }
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
