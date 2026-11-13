//! End-to-end tests of the core against real files.

use std::path::{Path, PathBuf};

use parvu_core::export::{ColumnSpec, ExportFormat, ExportOptions, Quoting};
use parvu_core::ops::{JoinType, Keep, Operation};
use parvu_core::search::{Direction, SearchOptions};
use parvu_core::stats::SampleMode;
use parvu_core::{CoreError, Workspace};
use tempfile::TempDir;

/// Write a file of `n` rows (id, name, score, flag, day) in the format of `path`.
fn make_file(path: &Path, n: u64) {
    let ws = Workspace::new(100, None).unwrap();
    let fmt = match path.extension().unwrap().to_str().unwrap() {
        "parquet" => "FORMAT PARQUET, ROW_GROUP_SIZE 1000",
        "csv" => "FORMAT CSV, HEADER",
        "json" => "FORMAT JSON",
        other => panic!("{other}"),
    };
    ws.connection()
        .execute_batch(&format!(
            "COPY (SELECT i AS id, 'name_' || (i % 7) AS name, i * 1.5 AS score, i % 2 = 0 AS flag, \
             DATE '2024-01-01' + CAST(i % 30 AS INTEGER) AS day FROM range({n}) t(i)) TO '{}' ({fmt})",
            path.display()
        ))
        .unwrap();
}

fn setup(n: u64, ext: &str) -> (TempDir, PathBuf, Workspace, u64) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("My Data.{ext}"));
    make_file(&path, n);
    let mut ws = Workspace::new(100, None).unwrap();
    let id = ws.open(&path).unwrap();
    (dir, path, ws, id)
}

fn cell(ws: &Workspace, id: u64, page: u64, row: usize, col: usize) -> Option<String> {
    ws.page(id, page).unwrap().rows[row][col].clone()
}

#[test]
fn opens_all_formats_and_pages() {
    for ext in ["parquet", "csv", "json"] {
        let (_d, _p, ws, id) = setup(250, ext);
        let s = ws.summary(id).unwrap();
        assert_eq!(s.name, "my_data", "{ext}");
        assert_eq!(s.total_rows, 250);
        assert_eq!(s.total_pages, 3);
        assert!(s.is_base_query);
        let p = ws.page(id, 3).unwrap();
        assert_eq!(p.offset, 200);
        assert_eq!(p.rows.len(), 50);
        assert_eq!(p.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["id", "name", "score", "flag", "day"]);
        assert_eq!(p.rows[0][0].as_deref(), Some("200"), "{ext}");
        assert_eq!(p.rows[0][4].as_deref(), Some("2024-01-21"), "{ext}");
        // Out-of-range pages clamp.
        assert_eq!(ws.page(id, 99).unwrap().page, 3);
    }
}

#[test]
fn duplicate_names_get_suffixes_and_close_drops_views() {
    let (_d, path, mut ws, id) = setup(10, "csv");
    let id2 = ws.open(&path).unwrap();
    assert_eq!(ws.summary(id2).unwrap().name, "my_data_2");
    ws.close(id).unwrap();
    assert!(ws.connection().execute_batch("SELECT * FROM my_data").is_err());
    assert!(ws.connection().execute_batch("SELECT * FROM my_data_2").is_ok());
    let id3 = ws.open(&path).unwrap();
    assert_eq!(ws.summary(id3).unwrap().name, "my_data");
}

#[test]
fn open_errors() {
    let mut ws = Workspace::new(100, None).unwrap();
    assert!(matches!(ws.open(Path::new("/nope/x.csv")), Err(CoreError::FileNotFound(_))));
    let d = TempDir::new().unwrap();
    let p = d.path().join("a.txt");
    std::fs::write(&p, "x").unwrap();
    assert!(matches!(ws.open(&p), Err(CoreError::UnsupportedFormat(_))));
    assert!(ws.tab_ids().is_empty());
}

#[test]
fn sql_ops_steps_and_undo() {
    let (_d, _p, mut ws, id) = setup(250, "parquet");
    ws.run_sql(id, "SELECT * FROM my_data WHERE id < 120;", None).unwrap();
    assert_eq!(ws.summary(id).unwrap().total_rows, 120);
    ws.apply(id, &Operation::Sort { column: "id".into(), ascending: false }).unwrap();
    assert_eq!(cell(&ws, id, 1, 0, 0).as_deref(), Some("119"));
    ws.apply(id, &Operation::Rename { column: "name".into(), new_name: "label".into() }).unwrap();
    ws.apply(id, &Operation::Remove { column: "flag".into() }).unwrap();
    let s = ws.summary(id).unwrap();
    assert_eq!(s.steps.iter().map(|l| l.key.as_str()).collect::<Vec<_>>(), ["step.query", "step.sort", "step.rename", "step.remove"]);
    assert!(!s.is_base_query);
    assert_eq!(ws.columns(id).unwrap().len(), 4);

    let undone = ws.undo(id).unwrap();
    assert_eq!(undone.key, "step.remove");
    assert_eq!(ws.columns(id).unwrap().len(), 5);
    ws.undo(id).unwrap();
    ws.undo(id).unwrap();
    ws.undo(id).unwrap();
    assert!(ws.summary(id).unwrap().is_base_query);
    assert_eq!(ws.summary(id).unwrap().total_rows, 250);
    assert!(matches!(ws.undo(id), Err(CoreError::NothingToUndo)));
}

#[test]
fn failed_query_changes_nothing() {
    let (_d, _p, mut ws, id) = setup(10, "csv");
    assert!(ws.run_sql(id, "SELECT nope FROM my_data", None).is_err());
    assert!(ws.run_sql(id, "   ", None).is_err());
    let s = ws.summary(id).unwrap();
    assert!(s.is_base_query);
    assert!(s.steps.is_empty());
}

#[test]
fn reset_keeps_cell_edits() {
    let (_d, _p, mut ws, id) = setup(10, "csv");
    ws.edit_cell(id, 0, "name", Some("edited".into())).unwrap();
    ws.apply(id, &Operation::Sort { column: "id".into(), ascending: false }).unwrap();
    ws.reset(id).unwrap();
    let s = ws.summary(id).unwrap();
    assert_eq!(s.steps.len(), 1);
    assert_eq!(s.steps[0].key, "step.cell_edit");
    assert_eq!(cell(&ws, id, 1, 0, 1).as_deref(), Some("edited"));
}

#[test]
fn drop_duplicates_keeps_order_and_last() {
    let (_d, _p, mut ws, id) = setup(21, "parquet");
    ws.apply(id, &Operation::DropDuplicates { columns: vec!["name".into()], keep: Keep::Last }).unwrap();
    let p = ws.page(id, 1).unwrap();
    assert_eq!(p.total_rows, 7);
    let ids: Vec<&str> = p.rows.iter().map(|r| r[0].as_deref().unwrap()).collect();
    assert_eq!(ids, ["14", "15", "16", "17", "18", "19", "20"]);
    ws.undo(id).unwrap();
    ws.apply(id, &Operation::DropDuplicates { columns: vec![], keep: Keep::First }).unwrap();
    assert_eq!(ws.summary(id).unwrap().total_rows, 21);
}

#[test]
fn filter_drop_null_change_type_duplicate_add() {
    let (_d, _p, mut ws, id) = setup(14, "csv");
    ws.apply(id, &Operation::FilterValues { column: "name".into(), values: vec![Some("name_1".into()), Some("name_2".into())] }).unwrap();
    assert_eq!(ws.summary(id).unwrap().total_rows, 4);
    ws.apply(id, &Operation::DropNull { column: "id".into(), null_value: Some("1".into()) }).unwrap();
    assert_eq!(ws.summary(id).unwrap().total_rows, 3);
    ws.apply(id, &Operation::ChangeType { column: "id".into(), new_type: "VARCHAR".into() }).unwrap();
    assert_eq!(ws.columns(id).unwrap()[0].dtype, "VARCHAR");
    ws.apply(id, &Operation::Duplicate { column: "score".into() }).unwrap();
    ws.apply(id, &Operation::AddColumn { name: "double".into(), expression: "\"my_data\".\"score\" * 2".into() }).unwrap();
    let cols: Vec<String> = ws.columns(id).unwrap().into_iter().map(|c| c.name).collect();
    assert_eq!(cols, ["id", "name", "score", "flag", "day", "score_copy", "double"]);
}

#[test]
fn relative_sql_sees_current_state() {
    let (_d, _p, mut ws, id) = setup(50, "parquet");
    ws.run_sql(id, "SELECT * FROM my_data WHERE id < 10", None).unwrap();
    let label = parvu_core::ops::StepLabel::new("step.add_column", &[("column", "x")]);
    ws.run_relative_sql(id, "SELECT *, my_data.id + 1 AS x FROM my_data", label).unwrap();
    let s = ws.summary(id).unwrap();
    assert_eq!(s.total_rows, 10);
    assert_eq!(cell(&ws, id, 1, 9, 5).as_deref(), Some("10"));
}

#[test]
fn join_and_append_use_each_tabs_current_state() {
    let d = TempDir::new().unwrap();
    let a = d.path().join("a.parquet");
    let b = d.path().join("b.csv");
    make_file(&a, 30);
    make_file(&b, 30);
    let mut ws = Workspace::new(100, None).unwrap();
    let ia = ws.open(&a).unwrap();
    let ib = ws.open(&b).unwrap();
    ws.run_sql(ib, "SELECT id, name AS other FROM b WHERE id < 5", None).unwrap();
    ws.apply(ia, &Operation::Join { right: "b".into(), join_type: JoinType::Inner, left_key: "id".into(), right_key: "id".into() }).unwrap();
    assert_eq!(ws.summary(ia).unwrap().total_rows, 5);
    ws.undo(ia).unwrap();
    ws.run_sql(ib, "SELECT * FROM b WHERE id < 3", None).unwrap();
    ws.apply(ia, &Operation::Append { tables: vec!["b".into()], distinct: false }).unwrap();
    assert_eq!(ws.summary(ia).unwrap().total_rows, 33);
}

#[test]
fn cell_edits_overlay_validate_and_undo() {
    let (_d, _p, mut ws, id) = setup(250, "parquet");
    let types_before = ws.columns(id).unwrap();
    assert!(ws.edit_cell(id, 205, "score", Some("3.5".into())).unwrap());
    assert!(ws.edit_cell(id, 205, "name", Some("changed".into())).unwrap());
    assert!(ws.edit_cell(id, 3, "flag", Some(String::new())).unwrap()); // empty -> NULL
    // Same value again is a no-op.
    assert!(!ws.edit_cell(id, 205, "name", Some("changed".into())).unwrap());
    // Invalid values are rejected with DuckDB's message and change nothing.
    assert!(ws.edit_cell(id, 1, "id", Some("abc".into())).is_err());

    let p = ws.page(id, 3).unwrap();
    assert_eq!(p.rows[5][2].as_deref(), Some("3.5"));
    assert_eq!(p.rows[5][1].as_deref(), Some("changed"));
    assert_eq!(p.rows[6][1].as_deref(), Some("name_3"));
    assert_eq!(p.edited, vec![[5, 2], [5, 1]]);
    assert_eq!(cell(&ws, id, 1, 3, 3), None);
    assert_eq!(ws.summary(id).unwrap().pending_edits, 3);
    // Edits are visible to SQL too, and types are unchanged.
    assert_eq!(ws.columns(id).unwrap(), types_before);

    ws.undo(id).unwrap();
    assert_eq!(cell(&ws, id, 1, 3, 3).as_deref(), Some("false"));
    // Editing is blocked once a transform is applied.
    ws.apply(id, &Operation::Sort { column: "id".into(), ascending: true }).unwrap();
    assert!(ws.edit_cell(id, 0, "name", Some("x".into())).is_err());
    assert!(!ws.page(id, 1).unwrap().editable);
}

#[test]
fn re_editing_keeps_original_old_value() {
    let (_d, _p, mut ws, id) = setup(5, "csv");
    ws.edit_cell(id, 1, "name", Some("a".into())).unwrap();
    ws.edit_cell(id, 1, "name", Some("b".into())).unwrap();
    let edits = ws.pending_edits(id).unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].old_value.as_deref(), Some("name_1"));
    assert_eq!(edits[0].new_value.as_deref(), Some("b"));
}

#[test]
fn row_order_is_stable_for_multi_row_group_parquet() {
    // Many row groups so DuckDB scans in parallel; edits and paging must
    // still address the same rows.
    let (_d, _p, mut ws, id) = setup(200_000, "parquet");
    ws.set_page_size(1000);
    ws.edit_cell(id, 150_123, "name", Some("needle".into())).unwrap();
    let p = ws.page(id, 151).unwrap();
    assert_eq!(p.rows[123][0].as_deref(), Some("150123"));
    assert_eq!(p.rows[123][1].as_deref(), Some("needle"));
    for (i, r) in p.rows.iter().enumerate() {
        assert_eq!(r[0].as_deref(), Some((150_000 + i).to_string().as_str()));
    }
    let hit = ws.find(id, &SearchOptions { query: "needle".into(), ..Default::default() }).unwrap().unwrap();
    assert_eq!((hit.row, hit.column_index), (150_123, 1));
}

#[test]
fn save_in_place_applies_edits_and_transforms() {
    for ext in ["parquet", "csv", "json"] {
        let (_d, path, mut ws, id) = setup(20, ext);
        ws.edit_cell(id, 2, "name", Some("saved".into())).unwrap();
        ws.apply(id, &Operation::Remove { column: "flag".into() }).unwrap();
        let rows = ws.save(id, None).unwrap();
        assert_eq!(rows, 20, "{ext}");
        let s = ws.summary(id).unwrap();
        assert!(s.steps.is_empty() && s.pending_edits == 0 && s.is_base_query, "{ext}");
        assert_eq!(ws.columns(id).unwrap().len(), 4, "{ext}");
        assert_eq!(cell(&ws, id, 1, 2, 1).as_deref(), Some("saved"), "{ext}");
        // No temporary files left behind.
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap()).unwrap().filter_map(|e| {
            let n = e.unwrap().file_name().to_string_lossy().to_string();
            n.starts_with('.').then_some(n)
        }).collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }
}

#[test]
fn save_as_other_format_repoints_tab() {
    let (d, _p, mut ws, id) = setup(20, "csv");
    let out = d.path().join("out.parquet");
    ws.edit_cell(id, 0, "score", Some("99.5".into())).unwrap();
    ws.save(id, Some(&out)).unwrap();
    let s = ws.summary(id).unwrap();
    assert_eq!(s.path, out);
    assert_eq!(cell(&ws, id, 1, 0, 2).as_deref(), Some("99.5"));
}

#[test]
fn excel_roundtrip() {
    let (d, _p, mut ws, id) = setup(30, "parquet");
    let xlsx = d.path().join("sheet.xlsx");
    let rows = ws.export(id, &ExportOptions::for_path(&xlsx)).unwrap();
    assert_eq!(rows, 30);
    let ix = ws.open(&xlsx).unwrap();
    let cols = ws.columns(ix).unwrap();
    let types: Vec<&str> = cols.iter().map(|c| c.dtype.as_str()).collect();
    // Dates are real Excel dates now, so they come back as DATE.
    assert_eq!(types, ["BIGINT", "VARCHAR", "DOUBLE", "BOOLEAN", "DATE"]);
    assert_eq!(cell(&ws, ix, 1, 3, 2).as_deref(), Some("4.5"));
    assert_eq!(cell(&ws, ix, 1, 3, 4).as_deref(), Some("2024-01-04"));
    // Edits and save back to Excel.
    ws.edit_cell(ix, 1, "name", Some("xl".into())).unwrap();
    ws.save(ix, None).unwrap();
    assert_eq!(cell(&ws, ix, 1, 1, 1).as_deref(), Some("xl"));
}

#[test]
fn export_options() {
    let (d, _p, ws, id) = setup(5, "parquet");
    let mut o = ExportOptions::for_path(d.path().join("o.csv"));
    o.columns = vec![
        ColumnSpec { name: "id".into(), source_type: "BIGINT".into(), include: true, cast_to: String::new() },
        ColumnSpec { name: "name".into(), source_type: "VARCHAR".into(), include: false, cast_to: String::new() },
        ColumnSpec { name: "day".into(), source_type: "DATE".into(), include: true, cast_to: String::new() },
    ];
    o.date_format = "%d/%m/%Y".into();
    o.delimiter = ";".into();
    o.quoting = Quoting::AllValid;
    ws.export(id, &o).unwrap();
    let text = std::fs::read_to_string(&o.path).unwrap();
    assert_eq!(text.lines().next().unwrap(), "\"id\";\"day\"");
    assert_eq!(text.lines().nth(2).unwrap(), "\"1\";\"02/01/2024\"");

    let mut o = ExportOptions::for_path(d.path().join("o.csv"));
    o.quoting = Quoting::None;
    o.include_header = false;
    ws.export(id, &o).unwrap();
    let text = std::fs::read_to_string(&o.path).unwrap();
    assert_eq!(text.lines().next().unwrap(), "0,name_0,0.0,true,2024-01-01");

    for (name, lines) in [("o.json", false), ("o.jsonl", true)] {
        let o = ExportOptions::for_path(d.path().join(name));
        assert_eq!(o.format, ExportFormat::Json);
        ws.export(id, &o).unwrap();
        let text = std::fs::read_to_string(&o.path).unwrap();
        assert_eq!(text.trim_start().starts_with('['), !lines, "{name}");
    }
    let mut o = ExportOptions::for_path(d.path().join("o.parquet"));
    o.compression = "zstd".into();
    assert_eq!(ws.export(id, &o).unwrap(), 5);
}

#[test]
fn stats_unique_tuple_and_search() {
    let (_d, _p, mut ws, id) = setup(100, "parquet");
    let s = ws.column_stats(id, "id").unwrap();
    assert_eq!((s.row_count, s.non_null, s.null, s.distinct), (100, 100, 0, 100));
    assert_eq!(s.min.as_deref(), Some("0"));
    assert_eq!(s.max.as_deref(), Some("99"));
    assert_eq!(s.mean, Some(49.5));
    let st = ws.column_stats(id, "name").unwrap();
    assert_eq!(st.mean, None);
    assert!(ws.column_stats(id, "nope").is_err());

    ws.edit_cell(id, 0, "name", None).unwrap();
    let u = ws.unique_values(id, "name").unwrap();
    assert_eq!(u.len(), 8);
    assert_eq!(u[0], None);
    assert_eq!(u[1].as_deref(), Some("name_0"));

    assert_eq!(ws.column_tuple(id, "id", SampleMode::First, 3).unwrap(), "(0, 1, 2)");
    assert_eq!(ws.column_tuple(id, "name", SampleMode::First, 2).unwrap(), "(None, 'name_1')");

    let mut o = SearchOptions { query: "NAME_3".into(), ..Default::default() };
    let h = ws.find(id, &o).unwrap().unwrap();
    assert_eq!((h.row, h.column.as_str()), (3, "name"));
    o.after_row = Some(3);
    o.after_col = Some(1);
    assert_eq!(ws.find(id, &o).unwrap().unwrap().row, 10);
    o.direction = Direction::Prev;
    o.after_row = Some(10);
    assert_eq!(ws.find(id, &o).unwrap().unwrap().row, 3);
    o.case_sensitive = true;
    o.after_row = None;
    assert!(ws.find(id, &o).unwrap().is_none());
    let o = SearchOptions { query: "^name_[56]$".into(), regex: true, whole_cell: true, column: Some("name".into()), ..Default::default() };
    assert_eq!(ws.find(id, &o).unwrap().unwrap().row, 5);
    let o = SearchOptions { query: "50%".into(), ..Default::default() };
    assert!(ws.find(id, &o).unwrap().is_none());
}

#[test]
fn excel_dates_and_timestamps_are_real_dates() {
    let (d, _p, mut ws, id) = setup(3, "parquet");
    ws.run_sql(
        id,
        "SELECT id, TIMESTAMP '2024-05-06 07:08:09' + to_hours(id) AS ts, DATE '1850-01-01' AS old_day FROM my_data",
        None,
    )
    .unwrap();
    let xlsx = d.path().join("ts.xlsx");
    ws.export(id, &ExportOptions::for_path(&xlsx)).unwrap();
    let ix = ws.open(&xlsx).unwrap();
    let types: Vec<String> = ws.columns(ix).unwrap().into_iter().map(|c| c.dtype).collect();
    // Excel cannot store dates before 1900, so those are written as text.
    assert_eq!(types, ["BIGINT", "TIMESTAMP", "VARCHAR"]);
    assert_eq!(cell(&ws, ix, 1, 1, 1).as_deref(), Some("2024-05-06 08:08:09"));
    assert_eq!(cell(&ws, ix, 1, 0, 2).as_deref(), Some("1850-01-01"));
}
