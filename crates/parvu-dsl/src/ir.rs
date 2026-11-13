//! Intermediate representation. Every DSL operation resolves to these nodes,
//! which the compiler emits as DuckDB SQL.

use serde::Serialize;

use crate::types::LogicalType;

/// Constant value of a [`Expr::Literal`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum LiteralValue {
    Null,
    Bool(bool),
    /// Arbitrary-precision integer as canonical decimal text (`-12`, `0`).
    Int(String),
    Float(f64),
    Str(String),
}

impl LiteralValue {
    pub fn int(v: i64) -> Self {
        LiteralValue::Int(v.to_string())
    }

    /// Parse a NUMBER token the way Python does: `float()` when it contains a
    /// dot, `int()` otherwise.
    pub fn from_number_token(text: &str) -> Self {
        if text.contains('.') {
            LiteralValue::Float(text.parse().unwrap_or(f64::NAN))
        } else {
            let (neg, digits) = match text.strip_prefix('-') {
                Some(rest) => (true, rest),
                None => (false, text),
            };
            let digits = digits.trim_start_matches('0');
            if digits.is_empty() {
                LiteralValue::Int("0".to_string())
            } else if neg {
                LiteralValue::Int(format!("-{digits}"))
            } else {
                LiteralValue::Int(digits.to_string())
            }
        }
    }
}

/// Resolved, typed expression.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "node", rename_all = "snake_case")]
pub enum Expr {
    /// `table[column]`.
    ColumnRef {
        table: String,
        column: String,
        logical_type: LogicalType,
    },
    Literal {
        value: LiteralValue,
        logical_type: LogicalType,
    },
    /// `FUNC(arg, ...)`.
    Call {
        func_name: String,
        args: Vec<Expr>,
        logical_type: LogicalType,
    },
    /// `op` is one of `+ - * / == = != <> < <= > >= AND OR`.
    BinaryOp {
        op: String,
        left: Box<Expr>,
        right: Box<Expr>,
        logical_type: LogicalType,
    },
    /// `op` is one of `+ - NOT`.
    UnaryOp {
        op: String,
        operand: Box<Expr>,
        logical_type: LogicalType,
    },
    /// `receiver.method(args...)`.
    MethodCall {
        receiver: Box<Expr>,
        method_name: String,
        args: Vec<Expr>,
        logical_type: LogicalType,
    },
    /// `table[column] = value`.
    Assignment {
        table: String,
        column: String,
        value: Box<Expr>,
        logical_type: LogicalType,
    },
    /// `drop_duplicates(table[c1], ..., keep)`; `keep` is `first` or `last`.
    DropDuplicates {
        table: String,
        columns: Vec<String>,
        keep: String,
        logical_type: LogicalType,
    },
    /// `REPLACE(text, pattern, with_value, case_sensitive, regex)`.
    Replace {
        text: Box<Expr>,
        pattern: Box<Expr>,
        with_value: Box<Expr>,
        case_sensitive: bool,
        regex: bool,
        logical_type: LogicalType,
    },
    /// Drop rows where `column` is NULL (`null_value` = `Null`) or equals the
    /// sentinel `null_value` (real NULLs are then kept).
    DropNull {
        table: String,
        column: String,
        null_value: LiteralValue,
        logical_type: LogicalType,
    },
}

impl Expr {
    pub fn logical_type(&self) -> LogicalType {
        match self {
            Expr::ColumnRef { logical_type, .. }
            | Expr::Literal { logical_type, .. }
            | Expr::Call { logical_type, .. }
            | Expr::BinaryOp { logical_type, .. }
            | Expr::UnaryOp { logical_type, .. }
            | Expr::MethodCall { logical_type, .. }
            | Expr::Assignment { logical_type, .. }
            | Expr::DropDuplicates { logical_type, .. }
            | Expr::Replace { logical_type, .. }
            | Expr::DropNull { logical_type, .. } => *logical_type,
        }
    }

    /// True for nodes that compile to a whole `SELECT` statement.
    pub fn is_table_level(&self) -> bool {
        matches!(self, Expr::DropDuplicates { .. } | Expr::DropNull { .. })
    }

    // ---- convenience constructors (logical type UNKNOWN unless obvious) ----

    pub fn column(table: &str, column: &str) -> Self {
        Expr::ColumnRef {
            table: table.into(),
            column: column.into(),
            logical_type: LogicalType::Unknown,
        }
    }

    pub fn literal(value: LiteralValue) -> Self {
        Expr::Literal {
            value,
            logical_type: LogicalType::Unknown,
        }
    }

    pub fn string(s: &str) -> Self {
        Expr::Literal {
            value: LiteralValue::Str(s.into()),
            logical_type: LogicalType::Text,
        }
    }

    pub fn int(v: i64) -> Self {
        Expr::Literal {
            value: LiteralValue::int(v),
            logical_type: LogicalType::Numeric,
        }
    }

    pub fn float(v: f64) -> Self {
        Expr::Literal {
            value: LiteralValue::Float(v),
            logical_type: LogicalType::Numeric,
        }
    }

    pub fn boolean(v: bool) -> Self {
        Expr::Literal {
            value: LiteralValue::Bool(v),
            logical_type: LogicalType::Boolean,
        }
    }

    pub fn null() -> Self {
        Expr::literal(LiteralValue::Null)
    }

    pub fn call(func_name: &str, args: Vec<Expr>) -> Self {
        Expr::Call {
            func_name: func_name.into(),
            args,
            logical_type: LogicalType::Unknown,
        }
    }

    pub fn binary(op: &str, left: Expr, right: Expr) -> Self {
        Expr::BinaryOp {
            op: op.into(),
            left: Box::new(left),
            right: Box::new(right),
            logical_type: LogicalType::Unknown,
        }
    }

    pub fn unary(op: &str, operand: Expr) -> Self {
        Expr::UnaryOp {
            op: op.into(),
            operand: Box::new(operand),
            logical_type: LogicalType::Unknown,
        }
    }

    pub fn method(receiver: Expr, method_name: &str, args: Vec<Expr>) -> Self {
        Expr::MethodCall {
            receiver: Box::new(receiver),
            method_name: method_name.into(),
            args,
            logical_type: LogicalType::Unknown,
        }
    }

    pub fn assignment(table: &str, column: &str, value: Expr) -> Self {
        Expr::Assignment {
            table: table.into(),
            column: column.into(),
            logical_type: value.logical_type(),
            value: Box::new(value),
        }
    }

    pub fn replace(
        text: Expr,
        pattern: Expr,
        with_value: Expr,
        case_sensitive: bool,
        regex: bool,
    ) -> Self {
        Expr::Replace {
            text: Box::new(text),
            pattern: Box::new(pattern),
            with_value: Box::new(with_value),
            case_sensitive,
            regex,
            logical_type: LogicalType::Text,
        }
    }

    pub fn drop_duplicates(table: &str, columns: Vec<String>, keep: &str) -> Self {
        Expr::DropDuplicates {
            table: table.into(),
            columns,
            keep: keep.into(),
            logical_type: LogicalType::Query,
        }
    }

    pub fn drop_null(table: &str, column: &str, null_value: LiteralValue) -> Self {
        Expr::DropNull {
            table: table.into(),
            column: column.into(),
            null_value,
            logical_type: LogicalType::Query,
        }
    }
}
