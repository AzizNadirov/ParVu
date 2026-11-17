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
    Formula { name: name.into(), source: sql.into(), sql: sql.into(), after: after.map(str::to_string) }
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
