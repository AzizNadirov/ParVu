//! Column formulas: columns computed by an expression of each row, kept
//! live as part of the table (between the cell edits and the steps).
//!
//! The expression arrives compiled (the app compiles the expression
//! language; this crate stays free of it). Its column references are
//! qualified with the table's name, so every layer aliases its input with
//! that name.

use serde::{Deserialize, Serialize};

use crate::page::ColumnMeta;
use crate::sql::quote_ident;

/// A computed column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Formula {
    /// The column it makes.
    pub name: String,
    /// What the user typed, e.g. `[qty] * [price]`.
    pub source: String,
    /// The compiled SQL expression.
    pub sql: String,
    /// Shown right after this column; at the end when `None` or unknown.
    #[serde(default)]
    pub after: Option<String>,
}

/// The table's view: `base` (the file with its cell edits) plus each
/// formula, in order. A formula sees the columns of `base` and of the
/// formulas before it, so formulas can't refer to each other in a cycle.
pub fn formula_sql(base: &str, table: &str, base_columns: &[ColumnMeta], formulas: &[Formula]) -> String {
    let mut query = format!("SELECT * FROM {}", quote_ident(base));
    if formulas.is_empty() {
        return query;
    }
    for f in formulas {
        query = format!("SELECT *, ({}) AS {} FROM ({query}) AS {}", f.sql, quote_ident(&f.name), quote_ident(table));
    }
    let mut order: Vec<&str> = base_columns.iter().map(|c| c.name.as_str()).collect();
    for f in formulas {
        let at = f.after.as_deref().and_then(|a| order.iter().position(|c| *c == a)).map(|i| i + 1).unwrap_or(order.len());
        order.insert(at, &f.name);
    }
    let list: Vec<String> = order.iter().map(|c| quote_ident(c)).collect();
    format!("SELECT {} FROM ({query})", list.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_and_order() {
        let cols = vec![ColumnMeta::new("qty", "BIGINT"), ColumnMeta::new("price", "DOUBLE")];
        assert_eq!(formula_sql("b", "t", &cols, &[]), "SELECT * FROM \"b\"");
        let f = |name: &str, sql: &str, after: Option<&str>| Formula {
            name: name.into(),
            source: String::new(),
            sql: sql.into(),
            after: after.map(str::to_string),
        };
        let sql = formula_sql("b", "t", &cols, &[f("total", "t.qty * t.price", None), f("half", "t.total / 2", Some("qty"))]);
        assert_eq!(
            sql,
            "SELECT \"qty\", \"half\", \"price\", \"total\" FROM (SELECT *, (t.total / 2) AS \"half\" FROM \
             (SELECT *, (t.qty * t.price) AS \"total\" FROM (SELECT * FROM \"b\") AS \"t\") AS \"t\")"
        );
    }
}
