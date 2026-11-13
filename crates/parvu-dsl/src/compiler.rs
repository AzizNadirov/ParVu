//! Compiler: IR -> DuckDB SQL.
//!
//! Reproduces the SQL text the Python compiler produced through sqlglot
//! (DuckDB dialect): identifier quoting rules, literal formatting and the
//! statement shapes. Unlike sqlglot it inserts parentheses where operator
//! precedence requires them, so the SQL keeps the meaning of the expression.

use crate::DslError;
use crate::ir::{Expr, LiteralValue};
use crate::registry::FunctionRegistry;

/// Compile an IR expression to a DuckDB SQL string.
pub fn compile(expr: &Expr, registry: &FunctionRegistry) -> Result<String, DslError> {
    Compiler { registry }.sql(expr)
}

fn cerr<T>(message: impl Into<String>) -> Result<T, DslError> {
    Err(DslError::Compile {
        message: message.into(),
    })
}

struct Compiler<'a> {
    registry: &'a FunctionRegistry,
}

/// Binding strength of an emitted SQL fragment (higher binds tighter).
const PREC_STATEMENT: u8 = 0;
const PREC_OR: u8 = 1;
const PREC_AND: u8 = 2;
const PREC_NOT: u8 = 3;
const PREC_CMP: u8 = 4;
const PREC_ADD: u8 = 5;
const PREC_MUL: u8 = 6;
const PREC_NEG: u8 = 7;
const PREC_ATOM: u8 = 9;

/// Normalised binary operator: (SQL symbol, precedence).
fn binary_op(op: &str) -> Option<(&'static str, u8)> {
    Some(match op.to_uppercase().as_str() {
        "+" => ("+", PREC_ADD),
        "-" => ("-", PREC_ADD),
        "*" => ("*", PREC_MUL),
        "/" => ("/", PREC_MUL),
        "=" | "==" => ("=", PREC_CMP),
        "<>" | "!=" => ("<>", PREC_CMP),
        "<" => ("<", PREC_CMP),
        "<=" => ("<=", PREC_CMP),
        ">" => (">", PREC_CMP),
        ">=" => (">=", PREC_CMP),
        "AND" => ("AND", PREC_AND),
        "OR" => ("OR", PREC_OR),
        _ => return None,
    })
}

fn precedence(e: &Expr) -> u8 {
    match e {
        Expr::BinaryOp { op, .. } => binary_op(op).map(|(_, p)| p).unwrap_or(PREC_ATOM),
        Expr::UnaryOp { op, operand, .. } => match op.to_uppercase().as_str() {
            "NOT" => PREC_NOT,
            "-" => PREC_NEG,
            "+" => precedence(operand),
            _ => PREC_ATOM,
        },
        Expr::Assignment { .. } | Expr::DropDuplicates { .. } | Expr::DropNull { .. } => {
            PREC_STATEMENT
        }
        _ => PREC_ATOM,
    }
}

fn paren(s: String, wrap: bool) -> String {
    if wrap { format!("({s})") } else { s }
}

impl Compiler<'_> {
    fn sql(&self, expr: &Expr) -> Result<String, DslError> {
        match expr {
            Expr::Assignment {
                table,
                column,
                value,
                ..
            } => Ok(format!(
                "SELECT *, {} AS {} FROM {}",
                self.sql(value)?,
                quoted_identifier(column),
                identifier(table)
            )),
            Expr::DropDuplicates {
                table,
                columns,
                keep,
                ..
            } => {
                let table = identifier(table);
                if keep == "first" && columns.is_empty() {
                    return Ok(format!("SELECT DISTINCT * FROM {table}"));
                }
                let mut over = Vec::new();
                if !columns.is_empty() {
                    let cols: Vec<String> = columns.iter().map(|c| identifier(c)).collect();
                    over.push(format!("PARTITION BY {}", cols.join(", ")));
                }
                if keep == "last" {
                    over.push("ORDER BY rowid DESC".to_string());
                }
                Ok(format!(
                    "SELECT * FROM {table} QUALIFY ROW_NUMBER() OVER ({}) = 1",
                    over.join(" ")
                ))
            }
            Expr::DropNull {
                table,
                column,
                null_value,
                ..
            } => {
                let table = identifier(table);
                let col = identifier(column);
                let filter = match null_value {
                    LiteralValue::Null => format!("NOT {col} IS NULL"),
                    v => format!("{col} IS NULL OR {col} <> {}", sentinel_sql(v)),
                };
                Ok(format!("SELECT * FROM {table} WHERE {filter}"))
            }
            Expr::Replace {
                text,
                pattern,
                with_value,
                case_sensitive,
                regex,
                ..
            } => {
                let text = self.sql(text)?;
                let replacement = self.sql(with_value)?;
                if !regex && *case_sensitive {
                    let pattern = self.sql(pattern)?;
                    return Ok(format!("REPLACE({text}, {pattern}, {replacement})"));
                }
                let flags = if *case_sensitive { "g" } else { "gi" };
                let pattern = match (&**pattern, regex) {
                    (
                        Expr::Literal {
                            value: LiteralValue::Str(s),
                            ..
                        },
                        false,
                    ) => string_literal(&python_re_escape(s)),
                    (p, _) => self.sql(p)?,
                };
                Ok(format!(
                    "REGEXP_REPLACE({text}, {pattern}, {replacement}, '{flags}')"
                ))
            }
            Expr::ColumnRef { table, column, .. } => {
                Ok(format!("{}.{}", identifier(table), identifier(column)))
            }
            Expr::Literal { value, .. } => Ok(literal_sql(value)),
            Expr::Call {
                func_name, args, ..
            } => {
                let name = match self.registry.lookup(func_name) {
                    Some(f) => f.sql_name.clone().unwrap_or_else(|| f.name.clone()),
                    None => func_name.clone(),
                };
                self.function(&name, args.iter())
            }
            Expr::MethodCall {
                receiver,
                method_name,
                args,
                ..
            } => {
                let Some(f) = self.registry.lookup_method(method_name) else {
                    return cerr(format!("Unknown method: {method_name}"));
                };
                let name = f.sql_name.clone().unwrap_or_else(|| f.name.clone());
                self.function(&name, std::iter::once(&**receiver).chain(args))
            }
            Expr::UnaryOp { op, operand, .. } => {
                let inner = self.sql(operand)?;
                let p = precedence(operand);
                match op.to_uppercase().as_str() {
                    "+" => Ok(inner),
                    "-" => {
                        let inner = paren(inner, p < PREC_NEG);
                        let sep = if inner.starts_with('-') { " " } else { "" };
                        Ok(format!("-{sep}{inner}"))
                    }
                    "NOT" => {
                        let wrap =
                            matches!(**operand, Expr::BinaryOp { .. }) || p == PREC_STATEMENT;
                        Ok(format!("NOT {}", paren(inner, wrap)))
                    }
                    _ => cerr(format!("Unknown unary operator: {op}")),
                }
            }
            Expr::BinaryOp {
                op, left, right, ..
            } => {
                let Some((sym, p)) = binary_op(op) else {
                    return cerr(format!("Unknown binary operator: {op}"));
                };
                let (lp, rp) = (precedence(left), precedence(right));
                let wrap_left = lp < p || (p == PREC_CMP && lp == PREC_CMP);
                let same_assoc_op = matches!(sym, "+" | "*" | "AND" | "OR")
                    && matches!(&**right, Expr::BinaryOp { op: rop, .. }
                        if binary_op(rop).map(|(s, _)| s) == Some(sym));
                let wrap_right = rp < p || (rp == p && !same_assoc_op);
                Ok(format!(
                    "{} {sym} {}",
                    paren(self.sql(left)?, wrap_left),
                    paren(self.sql(right)?, wrap_right)
                ))
            }
        }
    }

    fn function<'e>(
        &self,
        name: &str,
        args: impl Iterator<Item = &'e Expr>,
    ) -> Result<String, DslError> {
        let args: Vec<String> = args.map(|a| self.sql(a)).collect::<Result<_, _>>()?;
        Ok(format!("{name}({})", args.join(", ")))
    }
}

/// DuckDB reserved keywords as listed by sqlglot's DuckDB generator.
const RESERVED_KEYWORDS: &[&str] = &[
    "all",
    "analyse",
    "analyze",
    "and",
    "any",
    "array",
    "as",
    "asc_p",
    "asymmetric",
    "both",
    "case",
    "cast",
    "check_p",
    "collate",
    "column",
    "constraint",
    "create_p",
    "current_catalog",
    "current_date",
    "current_role",
    "current_time",
    "current_timestamp",
    "current_user",
    "default",
    "deferrable",
    "desc_p",
    "distinct",
    "do",
    "else",
    "end_p",
    "except",
    "false_p",
    "fetch",
    "for",
    "foreign",
    "from",
    "grant",
    "group_p",
    "having",
    "in_p",
    "initially",
    "intersect",
    "into",
    "lateral_p",
    "leading",
    "limit",
    "localtime",
    "localtimestamp",
    "not",
    "null_p",
    "offset",
    "on",
    "only",
    "or",
    "order",
    "placing",
    "primary",
    "references",
    "returning",
    "select",
    "session_user",
    "some",
    "symmetric",
    "table",
    "then",
    "to",
    "trailing",
    "true_p",
    "union",
    "unique",
    "user",
    "using",
    "variadic",
    "when",
    "where",
    "window",
    "with",
];

/// Python's `^[_a-zA-Z][\w]*$` (note: `$` also matches before a final `\n`).
fn is_safe_identifier(name: &str) -> bool {
    let body = name.strip_suffix('\n').unwrap_or(name);
    let mut chars = body.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_alphanumeric())
}

/// Identifier, quoted only when needed (unsafe characters or reserved word).
pub fn identifier(name: &str) -> String {
    if !is_safe_identifier(name) || RESERVED_KEYWORDS.contains(&name.to_lowercase().as_str()) {
        quoted_identifier(name)
    } else {
        name.to_string()
    }
}

/// Identifier, always double-quoted.
pub fn quoted_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Single-quoted SQL string literal.
pub fn string_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn literal_sql(v: &LiteralValue) -> String {
    match v {
        LiteralValue::Null => "NULL".into(),
        LiteralValue::Bool(true) => "TRUE".into(),
        LiteralValue::Bool(false) => "FALSE".into(),
        LiteralValue::Int(s) => s.clone(),
        LiteralValue::Float(f) => python_float_repr(*f),
        LiteralValue::Str(s) => string_literal(s),
    }
}

/// drop_null sentinel: sqlglot `Literal.number(str(v))` for non-strings, so
/// booleans render as `True` / `False`.
fn sentinel_sql(v: &LiteralValue) -> String {
    match v {
        LiteralValue::Bool(true) => "True".into(),
        LiteralValue::Bool(false) => "False".into(),
        other => literal_sql(other),
    }
}

/// Python 3.7+ `re.escape`.
pub fn python_re_escape(s: &str) -> String {
    const SPECIAL: &str = "()[]{}?*+-|^$\\.&~# \t\n\r\x0B\x0C";
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if SPECIAL.contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Python `repr(float)`.
pub fn python_float_repr(f: f64) -> String {
    if f.is_nan() {
        return "nan".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "inf".into() } else { "-inf".into() };
    }
    // Shortest round-trip digits, e.g. "-1.2345e3".
    let sci = format!("{f:e}");
    let (neg, sci) = match sci.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, sci.as_str()),
    };
    let (mantissa, exp) = sci.split_once('e').expect("LowerExp has an exponent");
    let exp: i32 = exp.parse().expect("integer exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let sign = if neg { "-" } else { "" };
    let body = if (-4..16).contains(&exp) {
        if exp >= 0 {
            let int_len = exp as usize + 1;
            if digits.len() <= int_len {
                format!("{digits}{}.0", "0".repeat(int_len - digits.len()))
            } else {
                format!("{}.{}", &digits[..int_len], &digits[int_len..])
            }
        } else {
            format!("0.{}{digits}", "0".repeat((-exp - 1) as usize))
        }
    } else {
        let frac = if digits.len() > 1 {
            format!("{}.{}", &digits[..1], &digits[1..])
        } else {
            digits.clone()
        };
        let esign = if exp < 0 { '-' } else { '+' };
        format!("{frac}e{esign}{:02}", exp.abs())
    };
    format!("{sign}{body}")
}

#[cfg(test)]
mod tests {
    //! Port of tests/unit/core/dsl/test_compiler.py.
    use super::*;

    fn c(e: Expr) -> String {
        compile(&e, &FunctionRegistry::default()).unwrap()
    }

    fn col(t: &str, c: &str) -> Expr {
        Expr::column(t, c)
    }

    #[test]
    fn column_ref() {
        assert_eq!(c(col("sales", "revenue")), "sales.revenue");
    }
    #[test]
    fn literal_string() {
        assert_eq!(c(Expr::string("hello")), "'hello'");
    }
    #[test]
    fn literal_number() {
        assert_eq!(c(Expr::int(42)), "42");
    }
    #[test]
    #[allow(clippy::approx_constant)]
    fn literal_float() {
        assert_eq!(c(Expr::float(3.14)), "3.14");
    }
    #[test]
    fn literal_boolean() {
        assert_eq!(c(Expr::boolean(true)), "TRUE");
    }
    #[test]
    fn literal_null() {
        assert_eq!(c(Expr::null()), "NULL");
    }
    #[test]
    fn call_upper() {
        assert_eq!(
            c(Expr::call("UPPER", vec![col("products", "name")])),
            "UPPER(products.name)"
        );
    }
    #[test]
    fn call_sum() {
        assert_eq!(
            c(Expr::call("SUM", vec![col("sales", "revenue")])),
            "SUM(sales.revenue)"
        );
    }
    #[test]
    fn call_if() {
        let e = Expr::call(
            "IF",
            vec![
                Expr::binary(">", col("sales", "revenue"), Expr::int(100)),
                Expr::string("high"),
                Expr::string("low"),
            ],
        );
        assert_eq!(c(e), "IF(sales.revenue > 100, 'high', 'low')");
    }
    #[test]
    fn binary_op_add() {
        assert_eq!(
            c(Expr::binary("+", col("sales", "a"), col("sales", "b"))),
            "sales.a + sales.b"
        );
    }
    #[test]
    fn binary_op_and() {
        assert_eq!(
            c(Expr::binary(
                "AND",
                col("sales", "flag"),
                col("sales", "active")
            )),
            "sales.flag AND sales.active"
        );
    }
    #[test]
    fn method_call_upper() {
        assert_eq!(
            c(Expr::method(col("products", "name"), "upper", vec![])),
            "UPPER(products.name)"
        );
    }
    #[test]
    fn method_call_contains() {
        assert_eq!(
            c(Expr::method(
                col("products", "name"),
                "contains",
                vec![Expr::string("apple")]
            )),
            "CONTAINS(products.name, 'apple')"
        );
    }
    #[test]
    fn method_call_round() {
        assert_eq!(
            c(Expr::method(
                col("sales", "revenue"),
                "round",
                vec![Expr::int(2)]
            )),
            "ROUND(sales.revenue, 2)"
        );
    }
    #[test]
    fn complex_expression() {
        let e = Expr::binary(
            "/",
            Expr::call("SUM", vec![col("sales", "revenue")]),
            Expr::call("COUNT", vec![col("sales", "id")]),
        );
        assert_eq!(c(e), "SUM(sales.revenue) / COUNT(sales.id)");
    }
    #[test]
    fn unknown_function_compiles() {
        assert_eq!(c(Expr::call("FOOBAR", vec![Expr::int(1)])), "FOOBAR(1)");
    }
    #[test]
    fn unknown_method_raises() {
        let e = Expr::method(col("sales", "x"), "nonexistent", vec![]);
        assert!(matches!(
            compile(&e, &FunctionRegistry::default()),
            Err(DslError::Compile { .. })
        ));
    }
    #[test]
    fn assignment() {
        let e = Expr::assignment(
            "sales",
            "new_col",
            Expr::call("CONCAT", vec![col("sales", "name"), Expr::string("!")]),
        );
        assert_eq!(
            c(e),
            "SELECT *, CONCAT(sales.name, '!') AS \"new_col\" FROM sales"
        );
    }
    #[test]
    fn assignment_quoted_column() {
        assert_eq!(
            c(Expr::assignment("sales", "new col", Expr::int(42))),
            "SELECT *, 42 AS \"new col\" FROM sales"
        );
    }

    fn rep(pattern: &str, with: &str, cs: bool, rx: bool) -> String {
        c(Expr::replace(
            col("products", "name"),
            Expr::string(pattern),
            Expr::string(with),
            cs,
            rx,
        ))
    }

    #[test]
    fn replace_literal_case_sensitive() {
        assert_eq!(
            rep("old", "new", true, false),
            "REPLACE(products.name, 'old', 'new')"
        );
    }
    #[test]
    fn replace_literal_case_insensitive() {
        let s = rep("old", "new", false, false);
        assert!(s.contains("REGEXP_REPLACE"));
        assert!(s.contains("'old'"));
        assert!(s.contains("'gi'"));
    }
    #[test]
    fn replace_regex() {
        assert_eq!(
            rep("[0-9]+", "#", true, true),
            "REGEXP_REPLACE(products.name, '[0-9]+', '#', 'g')"
        );
    }
    #[test]
    fn replace_regex_case_insensitive() {
        assert_eq!(
            rep("[a-z]+", "#", false, true),
            "REGEXP_REPLACE(products.name, '[a-z]+', '#', 'gi')"
        );
    }
    #[test]
    fn replace_escapes_regex_metacharacters() {
        assert!(rep("a.b", "x", false, false).contains(r"'a\.b'"));
    }

    #[test]
    fn identifiers() {
        assert_eq!(identifier("abc"), "abc");
        assert_eq!(identifier("Mixed_1"), "Mixed_1");
        assert_eq!(identifier("café"), "café");
        assert_eq!(identifier("select"), "\"select\"");
        assert_eq!(identifier("Select"), "\"Select\"");
        assert_eq!(identifier("1a"), "\"1a\"");
        assert_eq!(identifier("a b"), "\"a b\"");
        assert_eq!(identifier("x\"y"), "\"x\"\"y\"");
        assert_eq!(identifier(""), "\"\"");
    }

    #[test]
    fn float_repr() {
        for (v, s) in [
            (1.5, "1.5"),
            (3.0, "3.0"),
            (-0.0, "-0.0"),
            (0.0, "0.0"),
            (1e-5, "1e-05"),
            (0.0001, "0.0001"),
            (1e16, "1e+16"),
            (1.5e16, "1.5e+16"),
            (1234567890123456.5, "1234567890123456.5"),
            (123.456, "123.456"),
            (1e300, "1e+300"),
            (0.1, "0.1"),
        ] {
            assert_eq!(python_float_repr(v), s);
        }
    }

    #[test]
    fn precedence_parens() {
        let a = || col("t", "a");
        let b = || col("t", "b");
        assert_eq!(
            c(Expr::binary("*", Expr::binary("+", a(), b()), Expr::int(2))),
            "(t.a + t.b) * 2"
        );
        assert_eq!(
            c(Expr::binary("-", a(), Expr::binary("-", b(), Expr::int(1)))),
            "t.a - (t.b - 1)"
        );
        assert_eq!(
            c(Expr::binary("+", a(), Expr::binary("+", b(), Expr::int(1)))),
            "t.a + t.b + 1"
        );
        assert_eq!(
            c(Expr::binary("+", Expr::binary("-", a(), b()), Expr::int(1))),
            "t.a - t.b + 1"
        );
        assert_eq!(
            c(Expr::binary(
                "==",
                Expr::binary(">", a(), Expr::int(1)),
                Expr::boolean(true)
            )),
            "(t.a > 1) = TRUE"
        );
        assert_eq!(
            c(Expr::binary("AND", Expr::binary("OR", a(), b()), a())),
            "(t.a OR t.b) AND t.a"
        );
        assert_eq!(
            c(Expr::binary("OR", a(), Expr::binary("AND", b(), a()))),
            "t.a OR t.b AND t.a"
        );
        assert_eq!(
            c(Expr::unary("-", Expr::binary("*", a(), b()))),
            "-(t.a * t.b)"
        );
        assert_eq!(
            c(Expr::unary("NOT", Expr::binary("AND", a(), b()))),
            "NOT (t.a AND t.b)"
        );
        assert_eq!(
            c(Expr::binary("AND", Expr::unary("NOT", a()), b())),
            "NOT t.a AND t.b"
        );
    }
}
