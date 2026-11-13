//! Logical type system: maps DuckDB physical types to a small set of
//! logical types, each with an associated method set.

use serde::Serialize;

/// Logical data types in the ParVu expression language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LogicalType {
    Unknown,
    /// INTEGER, BIGINT, DOUBLE, DECIMAL, ...
    Numeric,
    /// VARCHAR
    Text,
    Boolean,
    Date,
    Timestamp,
    /// `List<T>`
    List,
    /// `Struct{...}`
    Struct,
    /// Table-level expression (produces a full SELECT).
    Query,
}

impl LogicalType {
    /// All variants, in declaration order.
    pub const ALL: [LogicalType; 9] = [
        LogicalType::Unknown,
        LogicalType::Numeric,
        LogicalType::Text,
        LogicalType::Boolean,
        LogicalType::Date,
        LogicalType::Timestamp,
        LogicalType::List,
        LogicalType::Struct,
        LogicalType::Query,
    ];

    /// Upper-case name, e.g. `"NUMERIC"`.
    pub fn name(self) -> &'static str {
        match self {
            LogicalType::Unknown => "UNKNOWN",
            LogicalType::Numeric => "NUMERIC",
            LogicalType::Text => "TEXT",
            LogicalType::Boolean => "BOOLEAN",
            LogicalType::Date => "DATE",
            LogicalType::Timestamp => "TIMESTAMP",
            LogicalType::List => "LIST",
            LogicalType::Struct => "STRUCT",
            LogicalType::Query => "QUERY",
        }
    }
}

impl std::fmt::Display for LogicalType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Convert a DuckDB physical type name to a [`LogicalType`].
///
/// Parameters such as `DECIMAL(10,2)` are stripped before lookup.
pub fn duckdb_type_to_logical(physical: &str) -> LogicalType {
    let base = physical
        .split('(')
        .next()
        .unwrap_or("")
        .trim()
        .to_uppercase();
    match base.as_str() {
        "INTEGER" | "BIGINT" | "SMALLINT" | "TINYINT" | "DOUBLE" | "FLOAT" | "DECIMAL" => {
            LogicalType::Numeric
        }
        "VARCHAR" | "TEXT" => LogicalType::Text,
        "BOOLEAN" => LogicalType::Boolean,
        "DATE" => LogicalType::Date,
        "TIMESTAMP" | "TIMESTAMP WITH TIME ZONE" => LogicalType::Timestamp,
        "LIST" => LogicalType::List,
        "STRUCT" => LogicalType::Struct,
        _ => LogicalType::Unknown,
    }
}

/// Method names available on a given logical type (dot-call syntax).
pub fn methods_for_type(logical_type: LogicalType) -> Vec<&'static str> {
    let methods: &[&'static str] = match logical_type {
        LogicalType::Text => &[
            "upper",
            "lower",
            "len",
            "trim",
            "contains",
            "startswith",
            "endswith",
            "replace",
        ],
        LogicalType::Numeric => &["abs", "round", "floor", "ceil", "power", "sqrt"],
        LogicalType::Date => &["year", "month", "day", "add_days"],
        LogicalType::Boolean => &["if_true"],
        _ => &[],
    };
    methods.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_mapping() {
        assert_eq!(
            duckdb_type_to_logical("DECIMAL(10,2)"),
            LogicalType::Numeric
        );
        assert_eq!(duckdb_type_to_logical("varchar"), LogicalType::Text);
        assert_eq!(
            duckdb_type_to_logical("TIMESTAMP WITH TIME ZONE"),
            LogicalType::Timestamp
        );
        assert_eq!(
            duckdb_type_to_logical("STRUCT(a INTEGER)"),
            LogicalType::Struct
        );
        assert_eq!(duckdb_type_to_logical("INTEGER[]"), LogicalType::Unknown);
        assert_eq!(duckdb_type_to_logical("HUGEINT"), LogicalType::Unknown);
    }

    #[test]
    fn method_sets() {
        assert_eq!(methods_for_type(LogicalType::Boolean), vec!["if_true"]);
        assert!(methods_for_type(LogicalType::Query).is_empty());
    }
}
