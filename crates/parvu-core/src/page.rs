//! A page of results as the UI receives it: column metadata plus every cell
//! already formatted as text. Formatting happens here, with Arrow's own
//! formatter, so nested, decimal and temporal types display consistently
//! without any formatting code in the UI.

use duckdb::arrow::record_batch::RecordBatch;
use duckdb::arrow::util::display::{ArrayFormatter, FormatOptions};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnMeta {
    pub name: String,
    /// DuckDB type name, e.g. `BIGINT`, `VARCHAR`, `DECIMAL(18,4)`.
    pub dtype: String,
}

impl ColumnMeta {
    pub fn new(name: impl Into<String>, dtype: impl Into<String>) -> Self {
        Self { name: name.into(), dtype: dtype.into() }
    }

    /// True for numeric DuckDB types (the UI right-aligns these).
    pub fn is_numeric(&self) -> bool {
        let t = self.dtype.to_uppercase();
        [
            "TINYINT", "SMALLINT", "INTEGER", "BIGINT", "HUGEINT", "UTINYINT", "USMALLINT", "UINTEGER",
            "UBIGINT", "UHUGEINT", "FLOAT", "DOUBLE", "DECIMAL", "REAL",
        ]
        .iter()
        .any(|n| t == *n || t.starts_with(&format!("{n}(")))
    }
}

/// Cell text, `None` for SQL NULL.
pub type Cell = Option<String>;

#[derive(Debug, Clone, Serialize)]
pub struct Page {
    /// 1-based page number actually served (after clamping).
    pub page: u64,
    pub total_pages: u64,
    pub total_rows: u64,
    pub page_size: u64,
    /// Absolute index of the first row on this page.
    pub offset: u64,
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<Cell>>,
    /// `[row_in_page, column_index]` of cells showing a pending edit.
    pub edited: Vec<[u32; 2]>,
    /// Whether cells may be edited (only on the unmodified file view).
    pub editable: bool,
}

pub(crate) fn format_options() -> FormatOptions<'static> {
    FormatOptions::default()
        .with_display_error(true)
        .with_timestamp_format(Some("%Y-%m-%d %H:%M:%S%.f"))
        .with_timestamp_tz_format(Some("%Y-%m-%d %H:%M:%S%.f%:z"))
        .with_datetime_format(Some("%Y-%m-%d %H:%M:%S%.f"))
}

/// Format record batches row by row into text cells.
pub fn format_batches(batches: &[RecordBatch]) -> Result<Vec<Vec<Cell>>> {
    let opts = format_options();
    let mut rows = Vec::new();
    for batch in batches {
        let formatters = batch
            .columns()
            .iter()
            .map(|a| ArrayFormatter::try_new(a.as_ref(), &opts))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| CoreError::invalid(e.to_string()))?;
        for r in 0..batch.num_rows() {
            let row = batch
                .columns()
                .iter()
                .zip(&formatters)
                .map(|(arr, f)| if arr.is_null(r) { None } else { Some(f.value(r).to_string()) })
                .collect();
            rows.push(row);
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_detection() {
        assert!(ColumnMeta::new("a", "BIGINT").is_numeric());
        assert!(ColumnMeta::new("a", "DECIMAL(18,4)").is_numeric());
        assert!(!ColumnMeta::new("a", "VARCHAR").is_numeric());
        assert!(!ColumnMeta::new("a", "INTEGER[]").is_numeric());
        assert!(!ColumnMeta::new("a", "TIMESTAMP").is_numeric());
    }
}
