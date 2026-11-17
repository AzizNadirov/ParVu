//! Table operations open their result in a new tab (in-memory table).

use parvu_core::Workspace;
use parvu_core::ops::Operation;
use parvu_core::reshape::{AggFunc, Aggregation};
use parvu_core::workspace::{Derivation, Materialized, RESULT_ROW_LIMIT, materialize};
use tempfile::TempDir;

fn setup() -> (TempDir, Workspace, u64) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("orders.parquet");
    let mut ws = Workspace::new(1000, None).unwrap();
    ws.connection()
        .execute_batch(&format!(
            "COPY (SELECT * FROM (VALUES (1, 'north', 10), (2, 'south', 5), (3, 'north', 7)) t(id, region, qty)) TO '{}' (FORMAT PARQUET)",
            path.display()
        ))
        .unwrap();
    let id = ws.open(&path).unwrap();
    (dir, ws, id)
}

fn group_by_region() -> Operation {
    Operation::GroupBy {
        by: vec!["region".into()],
        aggregations: vec![Aggregation { column: "qty".into(), func: AggFunc::Sum, alias: None }],
    }
}

fn rows(ws: &Workspace, id: u64) -> Vec<Vec<Option<String>>> {
    ws.page(id, 1).unwrap().rows
}

/// Plan, compute on a second connection (as the app's job does), open.
fn make(ws: &mut Workspace, id: u64, d: Derivation) -> u64 {
    let plan = ws.plan_result(id, d).unwrap();
    let conn = ws.clone_connection().unwrap();
    assert_eq!(materialize(&conn, &plan).unwrap(), Materialized::Rows(2));
    ws.add_result(plan).unwrap()
}

fn result_tables(ws: &Workspace) -> i64 {
    ws.connection()
        .query_row("SELECT count(*) FROM duckdb_tables() WHERE table_name LIKE '__pv_res_%'", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn group_by_opens_a_new_tab_and_leaves_the_source_alone() {
    let (dir, mut ws, src) = setup();
    let res = make(&mut ws, src, Derivation::Op { op: group_by_region() });
    let s = ws.summary(res).unwrap();
    assert_eq!(s.name, "orders_by_region");
    assert!(s.path.as_os_str().is_empty());
    let origin = s.origin.unwrap();
    assert_eq!((origin.source_id, origin.source.as_str()), (src, "orders"));
    assert_eq!(rows(&ws, res), vec![vec![Some("north".into()), Some("17".into())], vec![Some("south".into()), Some("5".into())]]);
    // The source keeps its data and has no new step.
    assert_eq!(ws.summary(src).unwrap().total_rows, 3);
    assert!(ws.summary(src).unwrap().steps.is_empty());
    // The result is a table of its own: cells can be edited.
    assert!(ws.has_unsaved_changes(res).unwrap());
    assert!(ws.page(res, 1).unwrap().editable);
    ws.edit_cell(res, 1, "qty_sum", Some("6".to_string())).unwrap();
    assert_eq!(rows(&ws, res)[1][1].as_deref(), Some("6"));
    // Ctrl+S has nowhere to write; Save As makes it a file.
    assert!(ws.save(res, None).is_err());
    let out = dir.path().join("by_region.parquet");
    assert_eq!(ws.save(res, Some(&out)).unwrap(), 2);
    let s = ws.summary(res).unwrap();
    assert!(s.origin.is_none());
    assert_eq!(s.path, out);
    assert!(!ws.has_unsaved_changes(res).unwrap());
    // SUM of integers is a 128-bit HUGEINT; it is saved as a whole number.
    assert_eq!(rows(&ws, res)[1][1].as_deref(), Some("6"));
    assert_eq!(result_tables(&ws), 0, "the in-memory copy is dropped once saved");
}

#[test]
fn refresh_recomputes_from_the_source() {
    let (_d, mut ws, src) = setup();
    let res = make(&mut ws, src, Derivation::Op { op: group_by_region() });
    ws.edit_cell(src, 1, "qty", Some("50".to_string())).unwrap();
    let plan = ws.refresh_plan(res).unwrap();
    assert_eq!(plan.name, "orders_by_region");
    materialize(&ws.clone_connection().unwrap(), &plan).unwrap();
    ws.replace_result(res, plan).unwrap();
    assert_eq!(rows(&ws, res)[1][1].as_deref(), Some("50"));
    assert_eq!(result_tables(&ws), 1, "the old copy is dropped");
    // Without its source, a result can't be refreshed.
    ws.close(src).unwrap();
    assert!(ws.refresh_plan(res).is_err());
    ws.close(res).unwrap();
    assert_eq!(result_tables(&ws), 0);
}

#[test]
fn only_table_operations_and_queries_make_new_tables() {
    let (_d, mut ws, src) = setup();
    let filter = Operation::Duplicate { column: "qty".into() };
    assert!(ws.plan_result(src, Derivation::Op { op: filter }).is_err());
    assert!(ws.plan_result(src, Derivation::Sql { sql: "SELECT nope FROM orders".into() }).is_err());
    let res = make(&mut ws, src, Derivation::Sql { sql: "SELECT DISTINCT region FROM orders ORDER BY 1;".into() });
    assert_eq!(ws.summary(res).unwrap().name, "orders_query");
    let again = make(&mut ws, src, Derivation::Sql { sql: "SELECT DISTINCT region FROM orders".into() });
    assert_eq!(ws.summary(again).unwrap().name, "orders_query_2");
}

#[test]
fn a_result_over_the_limit_is_not_kept() {
    let (_d, ws, src) = setup();
    let sql = format!("SELECT * FROM range({})", RESULT_ROW_LIMIT + 10);
    let plan = ws.plan_result(src, Derivation::Sql { sql }).unwrap();
    assert_eq!(materialize(ws.connection(), &plan).unwrap(), Materialized::TooBig);
    assert_eq!(result_tables(&ws), 0);
}
