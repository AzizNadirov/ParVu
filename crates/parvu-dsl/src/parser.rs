//! Parser: source text -> [`Ast`].
//!
//! The AST mirrors the shape of the Lark tree the Python resolver walked.
//! [`Ast::rule`] returns the Lark rule name of each node.

use pest::Parser as _;
use pest::iterators::Pair;
use pest_derive::Parser;
use serde::Serialize;

use crate::DslError;

#[derive(Parser)]
#[grammar = "grammar.pest"]
struct DslGrammar;

/// Binary operators, named after the Lark rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BinOp {
    LogicalOr,
    LogicalAnd,
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Add,
    Sub,
    Mul,
    Div,
}

impl BinOp {
    pub fn rule(self) -> &'static str {
        match self {
            BinOp::LogicalOr => "logical_or",
            BinOp::LogicalAnd => "logical_and",
            BinOp::Eq => "eq",
            BinOp::Ne => "ne",
            BinOp::Gt => "gt",
            BinOp::Gte => "gte",
            BinOp::Lt => "lt",
            BinOp::Lte => "lte",
            BinOp::Add => "add",
            BinOp::Sub => "sub",
            BinOp::Mul => "mul",
            BinOp::Div => "div",
        }
    }
}

/// Prefix operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnOp {
    Plus,
    Minus,
    Not,
}

/// Parsed (unresolved) expression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "node", rename_all = "snake_case")]
pub enum Ast {
    /// Raw NUMBER token text, e.g. `-12.50`.
    Number(String),
    /// Raw STRING token text including its quotes.
    String(String),
    Boolean(bool),
    Null,
    /// `table[column]`; `column` is the raw IDENT or STRING token text.
    ColumnRef {
        table: String,
        column: String,
    },
    FuncCall {
        name: String,
        args: Vec<Ast>,
    },
    MethodCall {
        receiver: Box<Ast>,
        name: String,
        args: Vec<Ast>,
    },
    Binary {
        op: BinOp,
        left: Box<Ast>,
        right: Box<Ast>,
    },
    Unary {
        op: UnOp,
        operand: Box<Ast>,
    },
    /// `table[column] = value`; `column` is the raw token text.
    Assignment {
        table: String,
        column: String,
        value: Box<Ast>,
    },
}

impl Ast {
    /// Lark rule name of this node (`column_ref`, `add`, `number`, ...).
    pub fn rule(&self) -> &'static str {
        match self {
            Ast::Number(_) => "number",
            Ast::String(_) => "string",
            Ast::Boolean(_) => "boolean",
            Ast::Null => "null",
            Ast::ColumnRef { .. } => "column_ref",
            Ast::FuncCall { .. } => "func_call",
            Ast::MethodCall { .. } => "method_call",
            Ast::Binary { op, .. } => op.rule(),
            Ast::Unary { op, .. } => match op {
                UnOp::Plus => "unary_plus",
                UnOp::Minus => "unary_minus",
                UnOp::Not => "logical_not",
            },
            Ast::Assignment { .. } => "assignment",
        }
    }
}

/// Parse source text into an [`Ast`].
pub fn parse(src: &str) -> Result<Ast, DslError> {
    let mut pairs = DslGrammar::parse(Rule::start, src).map_err(|e| {
        let (line, column) = match e.line_col {
            pest::error::LineColLocation::Pos(p) => p,
            pest::error::LineColLocation::Span(p, _) => p,
        };
        DslError::Parse {
            message: e.variant.message().into_owned(),
            line,
            column,
        }
    })?;
    let start = pairs.next().expect("start rule");
    let inner = start.into_inner().next().expect("start has a body");
    Ok(build(inner))
}

/// Parse, returning `None` on failure (for autocomplete on partial input).
pub fn parse_partial(src: &str) -> Option<Ast> {
    parse(src).ok()
}

fn build(pair: Pair<'_, Rule>) -> Ast {
    match pair.as_rule() {
        Rule::assignment => {
            let mut it = pair.into_inner();
            let (table, column) = column_parts(it.next().unwrap());
            let value = build(it.next().unwrap());
            Ast::Assignment {
                table,
                column,
                value: Box::new(value),
            }
        }
        Rule::expr | Rule::primary => build(pair.into_inner().next().unwrap()),
        Rule::or_expr | Rule::and_expr | Rule::compare_expr | Rule::add_expr | Rule::mul_expr => {
            let mut it = pair.into_inner();
            let mut acc = build(it.next().unwrap());
            while let Some(op) = it.next() {
                let rhs = build(it.next().unwrap());
                acc = Ast::Binary {
                    op: bin_op(op.as_str()),
                    left: Box::new(acc),
                    right: Box::new(rhs),
                };
            }
            acc
        }
        Rule::unary_expr => {
            let mut it = pair.into_inner();
            let first = it.next().unwrap();
            if first.as_rule() == Rule::prefix_op {
                let op = match first.as_str() {
                    "+" => UnOp::Plus,
                    "-" => UnOp::Minus,
                    _ => UnOp::Not,
                };
                Ast::Unary {
                    op,
                    operand: Box::new(build(it.next().unwrap())),
                }
            } else {
                build(first)
            }
        }
        Rule::postfix => {
            let mut it = pair.into_inner();
            let mut acc = build(it.next().unwrap());
            for suffix in it {
                let mut s = suffix.into_inner();
                let name = s.next().unwrap().as_str().to_string();
                let args = s.next().map(call_args).unwrap_or_default();
                acc = Ast::MethodCall {
                    receiver: Box::new(acc),
                    name,
                    args,
                };
            }
            acc
        }
        Rule::func_call => {
            let mut it = pair.into_inner();
            let name = it.next().unwrap().as_str().to_string();
            let args = call_args(it.next().unwrap());
            Ast::FuncCall { name, args }
        }
        Rule::column_ref => {
            let (table, column) = column_parts(pair);
            Ast::ColumnRef { table, column }
        }
        Rule::number => Ast::Number(pair.as_str().to_string()),
        Rule::string => Ast::String(pair.as_str().to_string()),
        Rule::boolean => Ast::Boolean(pair.as_str() == "TRUE"),
        Rule::null => Ast::Null,
        other => unreachable!("unexpected grammar rule {other:?}"),
    }
}

fn call_args(pair: Pair<'_, Rule>) -> Vec<Ast> {
    pair.into_inner().map(build).collect()
}

fn column_parts(pair: Pair<'_, Rule>) -> (String, String) {
    let mut it = pair.into_inner();
    let table = it.next().unwrap().as_str().to_string();
    let column = it.next().unwrap().as_str().to_string();
    (table, column)
}

fn bin_op(s: &str) -> BinOp {
    match s {
        "||" | "OR" => BinOp::LogicalOr,
        "&&" | "AND" => BinOp::LogicalAnd,
        "==" => BinOp::Eq,
        "!=" | "<>" => BinOp::Ne,
        ">" => BinOp::Gt,
        ">=" => BinOp::Gte,
        "<" => BinOp::Lt,
        "<=" => BinOp::Lte,
        "+" => BinOp::Add,
        "-" => BinOp::Sub,
        "*" => BinOp::Mul,
        "/" => BinOp::Div,
        other => unreachable!("unknown operator {other}"),
    }
}

#[cfg(test)]
mod tests {
    //! Port of tests/unit/core/dsl/test_parser.py plus lexer edge cases.
    use super::*;

    fn rule(src: &str) -> &'static str {
        parse(src).unwrap().rule()
    }

    #[test]
    fn column_ref() {
        assert_eq!(rule("sales[revenue]"), "column_ref");
    }
    #[test]
    fn literal_number() {
        assert_eq!(rule("42"), "number");
    }
    #[test]
    fn literal_string() {
        assert_eq!(rule("'hello'"), "string");
    }
    #[test]
    fn literal_boolean() {
        assert_eq!(rule("TRUE"), "boolean");
    }
    #[test]
    fn literal_null() {
        assert_eq!(rule("NULL"), "null");
    }
    #[test]
    fn function_call_no_args() {
        assert_eq!(rule("TODAY()"), "func_call");
    }
    #[test]
    fn function_call_one_arg() {
        assert_eq!(rule("SUM(sales[revenue])"), "func_call");
    }
    #[test]
    fn function_call_multi_args() {
        assert_eq!(rule("IF(sales[revenue] > 100, 'high', 'low')"), "func_call");
    }
    #[test]
    fn binary_op_add() {
        assert_eq!(rule("sales[a] + sales[b]"), "add");
    }
    #[test]
    fn binary_op_div() {
        assert_eq!(rule("sales[revenue] / sales[count]"), "div");
    }
    #[test]
    fn method_call() {
        assert_eq!(rule("sales[name].upper"), "method_call");
    }
    #[test]
    fn complex_expression() {
        assert_eq!(rule("SUM(sales[revenue]) / COUNT(sales[id])"), "div");
    }
    #[test]
    fn parenthesized() {
        assert_eq!(rule("(sales[a] + sales[b])"), "add");
    }
    #[test]
    fn parse_error() {
        assert!(matches!(parse("@#$%^&*"), Err(DslError::Parse { .. })));
    }
    #[test]
    fn parse_partial_incomplete() {
        assert!(parse_partial("SUM(").is_none());
    }
    #[test]
    fn parse_partial_valid() {
        assert!(parse_partial("sales[revenue]").is_some());
    }
    #[test]
    fn assignment() {
        assert_eq!(rule("sales[new_col] = sales[revenue] + 100"), "assignment");
    }
    #[test]
    fn assignment_quoted_column() {
        assert_eq!(rule("sales[\"new col\"] = sales[revenue]"), "assignment");
    }
    #[test]
    fn drop_duplicates() {
        assert_eq!(
            rule("drop_duplicates(sales[id], sales[name], \"first\")"),
            "func_call"
        );
    }
    #[test]
    fn method_call_no_args() {
        assert_eq!(rule("sales[name].upper"), "method_call");
    }
    #[test]
    fn method_call_with_args() {
        assert_eq!(rule("sales[name].replace('old', 'new')"), "method_call");
    }
    #[test]
    fn method_call_with_empty_parens() {
        assert_eq!(rule("sales[name].upper()"), "method_call");
    }

    #[test]
    fn lexer_edge_cases() {
        assert_eq!(parse("-5").unwrap(), Ast::Number("-5".into()));
        assert_eq!(rule("- 5"), "unary_minus");
        assert_eq!(rule("sales[a] -5"), "sub");
        assert_eq!(rule("NOT sales[a]"), "logical_not");
        assert_eq!(rule("NOTE(1)"), "func_call");
        assert_eq!(rule("NOTsales[a]"), "column_ref");
        assert_eq!(rule("sales[a] ORsales[b]"), "logical_or");
        assert_eq!(rule("sales[TRUE]"), "column_ref");
        assert_eq!(rule("sales[a] == 1"), "eq");
        assert_eq!(rule("sales[a] = 1"), "assignment");
        assert!(parse("TRUEX").is_err());
        assert!(parse("NULLIF(1)").is_err());
        assert!(parse("true").is_err());
        assert!(parse("1e5").is_err());
        assert!(parse("sales[a] + sales[b] = 1").is_err());
        assert!(parse("").is_err());
    }

    #[test]
    fn parse_error_position() {
        match parse("sales[a] +\n  )") {
            Err(DslError::Parse { line, column, .. }) => assert_eq!((line, column), (2, 3)),
            other => panic!("{other:?}"),
        }
    }
}
