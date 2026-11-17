//! Excel workbooks with several sheets: open any sheet, change the sheets,
//! and save without losing the others.

use std::path::Path;

use parvu_core::Workspace;
use parvu_core::types::CsvOptions;
use parvu_core::workbook::{SheetOp, sheets};
use rust_xlsxwriter::Workbook;
use tempfile::TempDir;

/// q1 (2 rows), q2 (hidden, 1 row), totals.
fn book(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("sales.xlsx");
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("Q1").unwrap();
    ws.write_row(0, 0, ["item", "qty"]).unwrap();
    ws.write_string(1, 0, "A").unwrap();
    ws.write_number(1, 1, 2).unwrap();
    ws.write_string(2, 0, "B").unwrap();
    ws.write_number(2, 1, 5).unwrap();
    let ws = wb.add_worksheet();
    ws.set_name("Q2").unwrap();
    ws.set_hidden(true);
    ws.write_row(0, 0, ["item", "qty"]).unwrap();
    ws.write_string(1, 0, "C").unwrap();
    ws.write_number(1, 1, 9).unwrap();
    let ws = wb.add_worksheet();
    ws.set_name("Totals").unwrap();
    ws.write_row(0, 0, ["label", "value"]).unwrap();
    ws.write_string(1, 0, "all").unwrap();
    ws.write_number(1, 1, 16).unwrap();
    wb.save(&path).unwrap();
    path
}

fn cells(ws: &Workspace, id: u64) -> Vec<Vec<Option<String>>> {
    ws.page(id, 1).unwrap().rows
}

fn names(path: &Path) -> Vec<(String, bool)> {
    sheets(path).unwrap().into_iter().map(|s| (s.name, s.hidden)).collect()
}

#[test]
fn open_any_sheet_and_switch() {
    let dir = TempDir::new().unwrap();
    let path = book(&dir);
    let mut ws = Workspace::new(100, None).unwrap();
    let id = ws.open(&path).unwrap();
    let s = ws.summary(id).unwrap();
    assert_eq!((s.name.as_str(), s.sheet.as_deref(), s.total_rows), ("sales", Some("Q1"), 2));

    let v = ws.sheets(id).unwrap();
    let listed: Vec<_> = v.sheets.iter().map(|s| (s.name.as_str(), s.hidden)).collect();
    assert_eq!(listed, [("Q1", false), ("Q2", true), ("Totals", false)]);
    assert_eq!(v.current, "Q1");

    // Another sheet in a new tab gets its own table name.
    let t = ws.open_sheet(&path, &CsvOptions::default(), Some("Totals")).unwrap();
    assert_eq!(ws.summary(t).unwrap().name, "sales_totals");
    assert_eq!(cells(&ws, t), vec![vec![Some("all".into()), Some("16".into())]]);
    assert_eq!(ws.sheets(id).unwrap().open, ["Q1", "Totals"]);

    // Switching a tab's sheet starts it afresh.
    ws.apply(id, &parvu_core::ops::Operation::Sort { column: "qty".into(), ascending: false }).unwrap();
    ws.set_sheet(id, "Q2").unwrap();
    let s = ws.summary(id).unwrap();
    assert_eq!((s.sheet.as_deref(), s.total_rows, s.steps.len()), (Some("Q2"), 1, 0));
}

#[test]
fn save_keeps_every_sheet_with_the_changes() {
    let dir = TempDir::new().unwrap();
    let path = book(&dir);
    let mut ws = Workspace::new(100, None).unwrap();
    let id = ws.open(&path).unwrap();
    ws.edit_cell(id, 0, "qty", Some("3".into())).unwrap();
    ws.sheet_op(id, &SheetOp::Rename { sheet: "Q2".into(), name: "Second".into() }).unwrap();
    ws.sheet_op(id, &SheetOp::Show { sheet: "Second".into() }).unwrap();
    ws.sheet_op(id, &SheetOp::Hide { sheet: "Totals".into() }).unwrap();
    ws.sheet_op(id, &SheetOp::Duplicate { sheet: "Q1".into() }).unwrap();
    ws.sheet_op(id, &SheetOp::Rename { sheet: "Q1".into(), name: "Jan-Mar".into() }).unwrap();
    let s = ws.summary(id).unwrap();
    assert!(s.sheets_changed);
    assert_eq!(s.sheet.as_deref(), Some("Jan-Mar"), "the tab shows its sheet's new name");
    // The sheet open in a tab can't be deleted.
    assert!(ws.sheet_op(id, &SheetOp::Delete { sheet: "Jan-Mar".into() }).is_err());

    ws.save(id, None).unwrap();
    assert_eq!(
        names(&path),
        [("Jan-Mar".into(), false), ("Q1 (2)".into(), false), ("Second".into(), false), ("Totals".into(), true)]
    );
    let s = ws.summary(id).unwrap();
    assert_eq!((s.sheet.as_deref(), s.sheets_changed, s.pending_edits), (Some("Jan-Mar"), false, 0));
    // The edited sheet holds the edit; its copy holds the values as they were saved before.
    assert_eq!(cells(&ws, id)[0], vec![Some("A".into()), Some("3".into())]);
    ws.set_sheet(id, "Q1 (2)").unwrap();
    assert_eq!(cells(&ws, id)[0], vec![Some("A".into()), Some("2".into())]);
    ws.set_sheet(id, "Second").unwrap();
    assert_eq!(cells(&ws, id), vec![vec![Some("C".into()), Some("9".into())]]);
}

#[test]
fn deleting_and_restoring_a_sheet() {
    let dir = TempDir::new().unwrap();
    let path = book(&dir);
    let mut ws = Workspace::new(100, None).unwrap();
    let id = ws.open(&path).unwrap();
    ws.sheet_op(id, &SheetOp::Delete { sheet: "Totals".into() }).unwrap();
    assert!(ws.summary(id).unwrap().sheets_changed);
    // Restoring brings the plan back to the file: nothing left to save.
    ws.sheet_op(id, &SheetOp::Restore { sheet: "Totals".into() }).unwrap();
    assert!(!ws.summary(id).unwrap().sheets_changed);
    ws.sheet_op(id, &SheetOp::Delete { sheet: "Totals".into() }).unwrap();
    ws.save(id, None).unwrap();
    assert_eq!(names(&path), [("Q1".into(), false), ("Q2".into(), true)]);
    // Closing the last tab of a workbook drops unsaved sheet changes.
    ws.sheet_op(id, &SheetOp::Hide { sheet: "Q1".into() }).unwrap_err();
    ws.sheet_op(id, &SheetOp::Rename { sheet: "Q2".into(), name: "X".into() }).unwrap();
    ws.close(id).unwrap();
    let id = ws.open(&path).unwrap();
    assert!(!ws.summary(id).unwrap().sheets_changed);
}

#[test]
fn save_as_copy_and_old_excel_files() {
    let dir = TempDir::new().unwrap();
    let path = book(&dir);
    let mut ws = Workspace::new(100, None).unwrap();
    let id = ws.open(&path).unwrap();
    ws.sheet_op(id, &SheetOp::Rename { sheet: "Totals".into(), name: "Sum".into() }).unwrap();
    let copy = dir.path().join("copy.xlsx");
    ws.save(id, Some(&copy)).unwrap();
    assert_eq!(names(&copy).len(), 3);
    assert_eq!(names(&copy)[2].0, "Sum");
    assert_eq!(names(&path)[2].0, "Totals", "the original is untouched");
    // .xls can't be written: refused before anything happens.
    let err = ws.save(id, Some(&dir.path().join("old.xls"))).unwrap_err().to_string();
    assert!(err.contains(".xlsx"), "{err}");
    // A CSV target still writes the one table.
    let csv = dir.path().join("q1.csv");
    ws.save(id, Some(&csv)).unwrap();
    assert!(std::fs::read_to_string(&csv).unwrap().starts_with("item,qty"));
}
