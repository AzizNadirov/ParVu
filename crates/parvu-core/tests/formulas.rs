//! Column formulas: live computed columns between the cell edits and the steps.

use parvu_core::Workspace;
use parvu_core::clean::{Combine, CondOp, Condition};
use parvu_core::formula::Formula;
use parvu_core::ops::Operation;
use tempfile::TempDir;

fn open() -> (TempDir, Workspace, u64) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("sales.csv");
    std::fs::write(&path, "item,qty,price\nA,2,10\nB,5,3\nC,1,7.5\n").unwrap();
    let mut ws = Workspace::new(100, None).unwrap();
    let id = ws.open(&path).unwrap();
    (dir, ws, id)
}

/// SQL as the expression language compiles it (`sales.qty`).
fn f(name: &str, sql: &str, after: Option<&str>) -> Formula {
    Formula { name: name.into(), source: sql.into(), sql: sql.into(), after: after.map(str::to_string), totals: Vec::new() }
}

fn col(ws: &Workspace, id: u64, name: &str) -> Vec<Option<String>> {
    let page = ws.page(id, 1).unwrap();
    let i = page.columns.iter().position(|c| c.name == name).unwrap_or_else(|| panic!("no column {name}"));
    page.rows.into_iter().map(|r| r[i].clone()).collect()
}

fn names(ws: &Workspace, id: u64) -> Vec<String> {
    ws.columns(id).unwrap().into_iter().map(|c| c.name).collect()
}

fn s(v: &[&str]) -> Vec<Option<String>> {
    v.iter().map(|x| Some(x.to_string())).collect()
}

#[test]
fn formulas_are_live_and_keep_cells_editable() {
    let (_d, mut ws, id) = open();
    ws.set_formula(id, f("total", "sales.qty * sales.price", None)).unwrap();
    assert_eq!(names(&ws, id), ["item", "qty", "price", "total"]);
    assert_eq!(col(&ws, id, "total"), s(&["20.0", "15.0", "7.5"]));
    assert!(ws.summary(id).unwrap().is_base_query, "a formula is not a transform");
    assert_eq!(ws.summary(id).unwrap().formulas.len(), 1);

    // Editing a source cell updates the formula at once.
    ws.edit_cell(id, 0, "qty", Some("4".into())).unwrap();
    assert_eq!(col(&ws, id, "total")[0].as_deref(), Some("40.0"));
    // Formula cells are not edited directly.
    assert!(ws.edit_cell(id, 0, "total", Some("1".into())).is_err());
    assert!(ws.edit_cells(id, &[(1, "total".into(), Some("1".into()))]).is_err());

    // A formula after another one may use it; placed after a given column.
    ws.set_formula(id, f("half", "sales.total / 2", Some("qty"))).unwrap();
    assert_eq!(names(&ws, id), ["item", "qty", "half", "price", "total"]);
    assert_eq!(col(&ws, id, "half")[0].as_deref(), Some("20.0"));

    // Changing a formula keeps its place; undo brings back the old one.
    ws.set_formula(id, f("total", "sales.qty + sales.price", None)).unwrap();
    assert_eq!(col(&ws, id, "total")[0].as_deref(), Some("14.0"));
    assert_eq!(col(&ws, id, "half")[0].as_deref(), Some("7.0"), "the dependent formula follows");
    ws.undo(id).unwrap();
    assert_eq!(col(&ws, id, "total")[0].as_deref(), Some("40.0"));
    ws.undo(id).unwrap(); // half
    ws.undo(id).unwrap(); // the cell edit
    ws.undo(id).unwrap(); // total
    assert_eq!(names(&ws, id), ["item", "qty", "price"]);
}

#[test]
fn bad_formulas_change_nothing() {
    let (_d, mut ws, id) = open();
    ws.set_formula(id, f("total", "sales.qty * sales.price", None)).unwrap();
    // Forward and self references: only earlier columns are visible.
    assert!(ws.set_formula(id, f("a", "sales.b + 1", None)).is_err());
    assert!(ws.set_formula(id, f("total", "sales.total + 1", None)).is_err());
    assert!(ws.set_formula(id, f("qty", "1", None)).is_err(), "a column of the file");
    assert!(ws.set_formula(id, f(" ", "1", None)).is_err());
    assert!(ws.set_formula(id, f("x", "nonsense(", None)).is_err());
    assert_eq!(names(&ws, id), ["item", "qty", "price", "total"]);
    assert_eq!(ws.summary(id).unwrap().steps.len(), 1);
    assert_eq!(col(&ws, id, "total")[1].as_deref(), Some("15.0"), "the old formula still works");
}

#[test]
fn steps_see_formula_columns_and_saving_writes_values() {
    let (dir, mut ws, id) = open();
    ws.set_formula(id, f("total", "sales.qty * sales.price", None)).unwrap();
    let keep_big = Operation::Filter {
        conditions: vec![Condition {
            column: "total".into(),
            op: CondOp::Gt,
            value: "10".into(),
            value2: String::new(),
            case_sensitive: false,
        }],
        combine: Combine::And,
    };
    ws.apply(id, &keep_big).unwrap();
    assert_eq!(ws.summary(id).unwrap().total_rows, 2);
    assert!(ws.remove_formula(id, "total").is_err(), "the filter uses it");
    ws.undo(id).unwrap();
    ws.remove_formula(id, "total").unwrap();
    assert_eq!(names(&ws, id), ["item", "qty", "price"]);
    ws.undo(id).unwrap();

    let out = dir.path().join("out.csv");
    ws.save(id, Some(&out)).unwrap();
    assert!(ws.summary(id).unwrap().formulas.is_empty(), "the file now holds the values");
    assert_eq!(names(&ws, id), ["item", "qty", "price", "total"]);
    assert!(std::fs::read_to_string(&out).unwrap().contains("20.0"));
}

#[test]
fn formulas_work_on_parquet_and_stay_lazy() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("big.parquet");
    let ws0 = Workspace::new(100, None).unwrap();
    ws0.connection()
        .execute_batch(&format!(
            "COPY (SELECT i AS id, i % 97 AS qty FROM range(300000) t(i)) TO '{}' (FORMAT PARQUET)",
            path.display()
        ))
        .unwrap();
    let mut ws = Workspace::new(100, None).unwrap();
    let id = ws.open(&path).unwrap();
    // The name of a file-backed table is its slug ("big").
    ws.set_formula(id, f("double", "big.qty * 2", None)).unwrap();
    assert_eq!(ws.summary(id).unwrap().total_rows, 300_000);
    let last = ws.summary(id).unwrap().total_pages;
    let page = ws.page(id, last).unwrap();
    let row = page.rows.last().unwrap();
    assert_eq!(row[0].as_deref(), Some("299999"));
    assert_eq!(row[2].as_deref(), Some(((299_999 % 97) * 2).to_string().as_str()));
    // Edits still work on a Parquet file with a formula.
    ws.edit_cell(id, 0, "qty", Some("50".into())).unwrap();
    assert_eq!(ws.page(id, 1).unwrap().rows[0][2].as_deref(), Some("100"));
}

/// `= [qty] / SUM([qty])`: the total is computed once per rebuild and
/// follows cell edits.
#[test]
fn column_totals_are_constants_that_follow_edits() {
    use parvu_core::formula::total_marker;
    let (_d, mut ws, id) = open();
    let share = Formula {
        totals: vec!["SUM(sales.qty)".into()],
        ..f("share", &format!("sales.qty / ({})", total_marker(0)), None)
    };
    ws.set_formula(id, share).unwrap();
    assert_eq!(col(&ws, id, "share"), s(&["0.25", "0.625", "0.125"]));
    // The view holds a constant, not a rescan per page.
    let sql: String = ws
        .connection()
        .query_row("SELECT sql FROM duckdb_views() WHERE view_name = 'sales'", [], |r| r.get(0))
        .unwrap();
    assert!(sql.contains("CAST('8' AS HUGEINT)") || sql.contains("'8'"), "{sql}");
    // Edit qty 2 → 12: the total becomes 18.
    ws.edit_cell(id, 0, "qty", Some("12".into())).unwrap();
    let got = col(&ws, id, "share");
    assert_eq!(got[2].as_deref(), Some(&*format!("{}", 1.0 / 18.0)));
    // Undo goes back to the old total.
    ws.undo(id).unwrap();
    assert_eq!(col(&ws, id, "share")[2].as_deref(), Some("0.125"));
}

/// A total of an empty or all-NULL column is NULL, not an error.
#[test]
fn null_totals() {
    use parvu_core::formula::total_marker;
    let (_d, mut ws, id) = open();
    let g = Formula {
        totals: vec!["MAX(CASE WHEN sales.qty > 100 THEN sales.qty END)".into()],
        ..f("big", &format!("COALESCE(({}), -1)", total_marker(0)), None)
    };
    ws.set_formula(id, g).unwrap();
    assert_eq!(col(&ws, id, "big"), s(&["-1", "-1", "-1"]));
}

/// PREV keeps the file's row order under a sort step.
#[test]
fn previous_row_in_file_order() {
    let (_d, mut ws, id) = open();
    ws.set_formula(id, f("prev_qty", "LAG(sales.qty, 1) OVER ()", None)).unwrap();
    assert_eq!(col(&ws, id, "prev_qty"), vec![None, Some("2".into()), Some("5".into())]);
    ws.apply(id, &Operation::Sort { column: "qty".into(), ascending: true }).unwrap();
    // Sorted by qty: C(1, prev B=5), A(2, none), B(5, prev A=2).
    assert_eq!(col(&ws, id, "prev_qty"), vec![Some("5".into()), None, Some("2".into())]);
}

/// A formula with a window over groups (`COUNTIF([region], [region])`)
/// keeps the file's row order; PREV still reads the row above.
#[test]
fn windowed_formulas_keep_the_row_order() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("sales.parquet");
    let mut ws = Workspace::new(100, None).unwrap();
    ws.connection()
        .execute_batch(&format!(
            "COPY (SELECT i AS id, ['north','south','east','west'][1 + (hash(i) % 4)::INTEGER] AS region FROM range(200000) t(i)) \
             TO '{}' (FORMAT PARQUET, ROW_GROUP_SIZE 2048)",
            path.display()
        ))
        .unwrap();
    let id = ws.open(&path).unwrap();
    // A pending cell edit puts the edits overlay under the formulas.
    ws.edit_cell(id, 7, "region", Some("north".into())).unwrap();
    ws.set_formula(id, f("same", "COUNT(sales.region) OVER (PARTITION BY sales.region)", None)).unwrap();
    ws.set_formula(id, f("prev", "LAG(sales.id, 1) OVER ()", None)).unwrap();
    let ids: Vec<i64> = col(&ws, id, "id").into_iter().map(|v| v.unwrap().parse().unwrap()).collect();
    assert_eq!(ids, (0..100).collect::<Vec<i64>>(), "rows stay in the file's order");
    let prev = col(&ws, id, "prev");
    assert_eq!((prev[0].as_deref(), prev[1].as_deref(), prev[50].as_deref()), (None, Some("0"), Some("49")));
    let total: i64 = ws
        .connection()
        .query_row("SELECT sum(same) FROM (SELECT DISTINCT region, same FROM sales)", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, 200000);
}
