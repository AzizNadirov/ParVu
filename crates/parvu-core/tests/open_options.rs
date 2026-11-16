//! Opening CSV files with options: delimiter, header, rows to skip, decimal comma.

use parvu_core::Workspace;
use parvu_core::types::{CsvOptions, Reading};
use tempfile::TempDir;

fn file(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("report.csv");
    std::fs::write(&path, "Sales report\nExported 2024-05-01\nregion;amount;note\nNorth;2,5;x\nSouth;4,0;y\n").unwrap();
    path
}

fn opts() -> CsvOptions {
    CsvOptions { delimiter: Some(";".into()), header: Some(true), skip: Some(2), decimal_separator: Some(",".into()), encoding: None }
}

#[test]
fn opens_with_options_and_keeps_them() {
    let dir = TempDir::new().unwrap();
    let path = file(&dir);
    let mut ws = Workspace::new(100, None).unwrap();

    let preview = ws.preview_csv(&path, &opts(), 10).unwrap();
    let names: Vec<&str> = preview.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["region", "amount", "note"]);
    assert_eq!(preview.rows[0][1].as_deref(), Some("2.5"));
    assert!(preview.detected.is_some());

    let id = ws.open_with(&path, &opts()).unwrap();
    let cols: Vec<(String, String)> = ws.columns(id).unwrap().into_iter().map(|c| (c.name, c.dtype)).collect();
    assert_eq!(cols[1], ("amount".to_string(), "DOUBLE".to_string()));
    assert_eq!(ws.summary(id).unwrap().total_rows, 2);
    // "Read as" rebuilds the reader: the options must still apply.
    ws.set_source_reading(id, "amount", Reading::Text).unwrap();
    assert_eq!(ws.page(id, 1).unwrap().rows[0][1].as_deref(), Some("2,5"));
    ws.undo(id).unwrap();
    assert_eq!(ws.page(id, 1).unwrap().rows[1][1].as_deref(), Some("4.0"));
}

#[test]
fn bad_options_are_refused() {
    let dir = TempDir::new().unwrap();
    let path = file(&dir);
    let mut ws = Workspace::new(100, None).unwrap();
    let bad = |o: CsvOptions| ws.preview_csv(&path, &o, 5).is_err();
    assert!(bad(CsvOptions { decimal_separator: Some(";".into()), ..Default::default() }));
    assert!(bad(CsvOptions { encoding: Some("ebcdic').x".into()), ..Default::default() }));
    assert!(bad(CsvOptions { delimiter: Some(String::new()), ..Default::default() }));
    assert!(ws.open_with(&path, &CsvOptions { delimiter: Some("'); DROP".into()), ..Default::default() }).is_err());
}
