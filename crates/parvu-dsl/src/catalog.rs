//! Catalog: maps table names to column names and DuckDB physical types.
//!
//! Plain data. The application fills it from DuckDB (`DESCRIBE` or
//! `information_schema`) and calls [`Catalog::register_table`].

use serde::Serialize;

use crate::types::{LogicalType, duckdb_type_to_logical};

/// Metadata for a single column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ColumnInfo {
    pub name: String,
    pub physical_type: String,
    pub logical_type: LogicalType,
}

/// Catalog of tables and columns known to the DSL. Tables and columns keep
/// their registration order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Catalog {
    tables: Vec<(String, Vec<ColumnInfo>)>,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) a table. `columns` yields
    /// `(column_name, duckdb_physical_type)`. A re-registered table keeps its
    /// position; a repeated column name keeps its first position and the last
    /// type.
    pub fn register_table(
        &mut self,
        name: &str,
        columns: impl IntoIterator<Item = (String, String)>,
    ) {
        let mut cols: Vec<ColumnInfo> = Vec::new();
        for (col, phys) in columns {
            let info = ColumnInfo {
                logical_type: duckdb_type_to_logical(&phys),
                name: col.clone(),
                physical_type: phys,
            };
            match cols.iter_mut().find(|c| c.name == col) {
                Some(existing) => *existing = info,
                None => cols.push(info),
            }
        }
        match self.tables.iter_mut().find(|(n, _)| n == name) {
            Some(entry) => entry.1 = cols,
            None => self.tables.push((name.to_string(), cols)),
        }
    }

    pub fn unregister_table(&mut self, name: &str) {
        self.tables.retain(|(n, _)| n != name);
    }

    pub fn clear(&mut self) {
        self.tables.clear();
    }

    pub fn tables(&self) -> Vec<String> {
        self.tables.iter().map(|(n, _)| n.clone()).collect()
    }

    pub fn has_table(&self, table: &str) -> bool {
        self.tables.iter().any(|(n, _)| n == table)
    }

    pub fn columns(&self, table: &str) -> Vec<String> {
        self.table_columns(table)
            .map(|cols| cols.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default()
    }

    pub fn column_info(&self, table: &str, col: &str) -> Option<&ColumnInfo> {
        self.table_columns(table)?.iter().find(|c| c.name == col)
    }

    /// Logical type of a column, or `Unknown`.
    pub fn resolve_column(&self, table: &str, col: &str) -> LogicalType {
        self.column_info(table, col)
            .map(|c| c.logical_type)
            .unwrap_or(LogicalType::Unknown)
    }

    fn table_columns(&self, table: &str) -> Option<&[ColumnInfo]> {
        self.tables
            .iter()
            .find(|(n, _)| n == table)
            .map(|(_, c)| c.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn register_and_query() {
        let mut c = Catalog::new();
        c.register_table("sales", cols(&[("id", "INTEGER"), ("name", "VARCHAR")]));
        c.register_table("other", cols(&[]));
        assert_eq!(c.tables(), vec!["sales", "other"]);
        assert_eq!(c.columns("sales"), vec!["id", "name"]);
        assert_eq!(c.resolve_column("sales", "name"), LogicalType::Text);
        assert_eq!(c.resolve_column("sales", "zzz"), LogicalType::Unknown);
        assert!(c.column_info("nope", "id").is_none());
        c.register_table("sales", cols(&[("x", "DATE")]));
        assert_eq!(c.tables(), vec!["sales", "other"]);
        assert_eq!(c.columns("sales"), vec!["x"]);
        c.unregister_table("sales");
        assert_eq!(c.tables(), vec!["other"]);
    }
}
