//! Pending cell edits (port of `core/edit_queue.py`).
//!
//! Edits are keyed by absolute row in the source file's natural order and by
//! column name. Instead of loading the file into memory to apply them (as the
//! Python app did with pandas), [`overlay_sql`] builds a view that replaces
//! edited cells on the fly. Queries, joins and saves all read through it.

use duckdb::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::page::{Cell, ColumnMeta};
use crate::sql::{is_safe_type_name, quote_ident, quote_literal};

/// Hidden row-number column used by the overlay.
pub const ROW_COLUMN: &str = "__pv_row";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellEdit {
    /// 0-based absolute row in the unmodified file view.
    pub row: u64,
    pub column: String,
    pub old_value: Cell,
    pub new_value: Cell,
}

#[derive(Debug, Clone, Default)]
pub struct EditQueue {
    edits: Vec<CellEdit>,
}

impl EditQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an edit. Editing the same cell again keeps the original `old_value`.
    pub fn add(&mut self, edit: CellEdit) {
        match self.edits.iter_mut().find(|e| e.row == edit.row && e.column == edit.column) {
            Some(existing) => existing.new_value = edit.new_value,
            None => self.edits.push(edit),
        }
    }

    pub fn remove(&mut self, row: u64, column: &str) -> Option<CellEdit> {
        let idx = self.edits.iter().position(|e| e.row == row && e.column == column)?;
        Some(self.edits.remove(idx))
    }

    pub fn get(&self, row: u64, column: &str) -> Option<&CellEdit> {
        self.edits.iter().find(|e| e.row == row && e.column == column)
    }

    pub fn contains(&self, row: u64, column: &str) -> bool {
        self.get(row, column).is_some()
    }

    pub fn is_dirty(&self) -> bool {
        !self.edits.is_empty()
    }

    pub fn clear(&mut self) {
        self.edits.clear();
    }

    pub fn all(&self) -> &[CellEdit] {
        &self.edits
    }

    /// Number of distinct edited cells.
    pub fn len(&self) -> usize {
        self.edits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }
}

/// SQL that reads `source` with `edits` applied. Column order and types are
/// unchanged; each edited value is cast to its column's type.
pub fn overlay_sql(source: &str, columns: &[ColumnMeta], edits: &[CellEdit]) -> Result<String> {
    let src = quote_ident(source);
    if edits.is_empty() {
        return Ok(format!("SELECT * FROM {src}"));
    }
    let mut replaces = Vec::new();
    for col in columns {
        let mine: Vec<&CellEdit> = edits.iter().filter(|e| e.column == col.name).collect();
        if mine.is_empty() {
            continue;
        }
        if !is_safe_type_name(&col.dtype) {
            return Err(CoreError::invalid(format!("Cannot edit column of type {}", col.dtype)));
        }
        let ident = quote_ident(&col.name);
        let mut case = format!("CASE {ROW_COLUMN}");
        for e in mine {
            let value = match &e.new_value {
                Some(v) => format!("CAST({} AS {})", quote_literal(v), col.dtype),
                None => format!("CAST(NULL AS {})", col.dtype),
            };
            case.push_str(&format!(" WHEN {} THEN {value}", e.row));
        }
        case.push_str(&format!(" ELSE {ident} END AS {ident}"));
        replaces.push(case);
    }
    if let Some(e) = edits.iter().find(|e| !columns.iter().any(|c| c.name == e.column)) {
        return Err(CoreError::UnknownColumn(e.column.clone()));
    }
    Ok(format!(
        "SELECT * EXCLUDE ({ROW_COLUMN}) REPLACE ({}) FROM (SELECT *, ROW_NUMBER() OVER () - 1 AS {ROW_COLUMN} FROM {src})",
        replaces.join(", ")
    ))
}

/// Check that `value` can be stored in a column of DuckDB type `dtype`.
/// Returns DuckDB's own conversion error message on failure.
pub fn validate_value(conn: &Connection, dtype: &str, value: &Cell) -> Result<()> {
    let Some(v) = value else { return Ok(()) };
    if !is_safe_type_name(dtype) {
        return Err(CoreError::invalid(format!("Cannot edit column of type {dtype}")));
    }
    conn.query_row(&format!("SELECT CAST(? AS {dtype}) IS NULL"), [v], |r| r.get::<_, bool>(0))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(row: u64, col: &str, old: &str, new: &str) -> CellEdit {
        CellEdit { row, column: col.into(), old_value: Some(old.into()), new_value: Some(new.into()) }
    }

    #[test]
    fn re_edit_keeps_original_old_value() {
        let mut q = EditQueue::new();
        q.add(edit(1, "a", "x", "y"));
        q.add(edit(1, "a", "y", "z"));
        assert_eq!(q.len(), 1);
        let e = q.get(1, "a").unwrap();
        assert_eq!(e.old_value.as_deref(), Some("x"));
        assert_eq!(e.new_value.as_deref(), Some("z"));
    }

    #[test]
    fn remove_and_dirty() {
        let mut q = EditQueue::new();
        assert!(!q.is_dirty());
        q.add(edit(1, "a", "x", "y"));
        q.add(edit(2, "a", "x", "y"));
        assert!(q.is_dirty());
        assert!(q.remove(1, "a").is_some());
        assert!(q.remove(1, "a").is_none());
        assert!(!q.contains(1, "a"));
        assert!(q.contains(2, "a"));
        q.clear();
        assert!(q.is_empty());
    }

    #[test]
    fn overlay_without_edits_is_plain_select() {
        assert_eq!(overlay_sql("src", &[], &[]).unwrap(), "SELECT * FROM \"src\"");
    }

    #[test]
    fn overlay_rejects_unknown_column() {
        let cols = vec![ColumnMeta::new("a", "BIGINT")];
        let err = overlay_sql("src", &cols, &[edit(0, "b", "1", "2")]).unwrap_err();
        assert!(matches!(err, CoreError::UnknownColumn(_)));
    }
}
