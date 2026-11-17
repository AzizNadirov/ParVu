//! Custom functions in SQL mode and in read-only queries
//! (`parvu_core::custom::sqlmode`).

use std::sync::Arc;

use parvu_core::custom::{Definition, Library, ParamKind, ValidateEnv, samples, sqlmode, validate};
use parvu_core::workspace::{Derivation, materialize};
use parvu_core::{TabId, Workspace};

fn checked(mut d: Definition) -> Definition {
    let env = ValidateEnv { name_taken: &|_| None, app_version: "test", expression_sql: None };
    let r = validate(&d, &env).unwrap();
    assert!(r.passed(), "{}: {:#?}", d.name, r.checks);
    d.compiled = r.compiled;
    d
}

fn library() -> Arc<Library> {
    Arc::new(Library::new(samples().into_iter().map(checked)))
}

const CSV: &str = "Phone,region,price,qty\n\
+1 (555) 010-2030,north,10,1\n\
n/a,north,20,1\n\
555.0102,south,30,2\n";

fn workspace() -> (tempfile::TempDir, Workspace, TabId) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("contacts.csv");
    std::fs::write(&path, CSV).unwrap();
    let mut ws = Workspace::new(100, None).unwrap();
    ws.set_custom(library());
    let id = ws.open(&path).unwrap();
    (dir, ws, id)
}

/// Every row of the tab's current result, as text.
fn rows(ws: &Workspace, id: TabId) -> Vec<Vec<String>> {
    let page = ws.page(id, 1).unwrap();
    page.rows.iter().map(|r| r.iter().map(|c| c.clone().unwrap_or_else(|| "∅".into())).collect()).collect()
}

fn names(ws: &Workspace, id: TabId) -> Vec<String> {
    ws.columns(id).unwrap().into_iter().map(|c| c.name).collect()
}

fn expand(ws: &Workspace, sql: &str) -> String {
    ws.expand_custom(sql).unwrap().sql
}

fn refused(ws: &Workspace, sql: &str) -> String {
    ws.expand_custom(sql).unwrap_err().to_string()
}

#[test]
fn a_row_function_in_select_and_where() {
    let (_d, mut ws, id) = workspace();
    let label = ws
        .run_sql(id, "SELECT clean_phone(Phone) AS p, CLEAN_PHONE(Phone, '00') AS q FROM contacts WHERE clean_phone(Phone) IS NOT NULL", None)
        .unwrap();
    assert_eq!(rows(&ws, id), [["+15550102030", "0015550102030"], ["+5550102", "005550102"]]);
    // The step keeps the expanded SQL, so it doesn't depend on the definition.
    let step = ws.current_query(id).unwrap();
    assert!(!step.to_lowercase().contains("clean_phone(") && step.contains("regexp_replace"), "{step}");
    assert!(label.args["sql"].contains("clean_phone(") && !label.args["sql"].contains("regexp_replace"), "the label shows what was typed: {label:?}");
    ws.set_custom(Arc::new(Library::default()));
    assert_eq!(rows(&ws, id).len(), 2);
    ws.undo(id).unwrap();
    assert_eq!(rows(&ws, id).len(), 3);
}

#[test]
fn unaliased_calls_keep_the_name_duckdb_gives_them() {
    let (_d, mut ws, id) = workspace();
    ws.run_sql(id, "SELECT clean_phone(Phone), upper(clean_phone(Phone)) FROM contacts", None).unwrap();
    assert_eq!(names(&ws, id), ["clean_phone(Phone)", "upper(clean_phone(Phone))"]);
}

#[test]
fn nested_calls() {
    let (_d, mut ws, id) = workspace();
    // A custom call inside a built-in, a built-in inside a custom argument,
    // and a custom call inside a custom argument.
    ws.run_sql(
        id,
        "SELECT length(clean_phone(Phone)) AS n, clean_phone(lower(Phone), upper('x')) AS p, \
         title_case(clean_phone(region || ' 1234567')) AS t FROM contacts LIMIT 1",
        None,
    )
    .unwrap();
    assert_eq!(rows(&ws, id), [["12", "X15550102030", "+1234567"]]);
}

#[test]
fn a_summary_function_in_group_by() {
    let (_d, mut ws, id) = workspace();
    ws.run_sql(id, "SELECT region, weighted_average(price, qty) AS wa FROM contacts GROUP BY region ORDER BY region", None)
        .unwrap();
    assert_eq!(rows(&ws, id), [["north", "15.0"], ["south", "30.0"]]);
    // Over the whole table, with FILTER given to each aggregate.
    ws.run_sql(id, "SELECT weighted_average(price, qty) FILTER (WHERE region = 'north') AS wa FROM contacts", None).unwrap();
    assert_eq!(rows(&ws, id), [["15.0"]]);
}

#[test]
fn a_summary_function_with_over() {
    let (_d, mut ws, id) = workspace();
    // weighted_average is sum(x * w) / sum(…): OVER goes to both sums.
    let sql = "SELECT region, price, weighted_average(price, qty) OVER (PARTITION BY region) AS wa, \
               weighted_average(price, qty) OVER () AS total FROM contacts ORDER BY price";
    let x = expand(&ws, sql);
    assert_eq!(x.matches("OVER (PARTITION BY region)").count(), 2, "{x}");
    ws.run_sql(id, sql, None).unwrap();
    assert_eq!(rows(&ws, id), [["north", "10", "15.0", "22.5"], ["north", "20", "15.0", "22.5"], ["south", "30", "30.0", "22.5"]]);
    // A row function takes no OVER.
    assert!(refused(&ws, "SELECT clean_phone(Phone) OVER () FROM contacts").contains("row function"));
}

#[test]
fn optional_values_and_arity() {
    let (_d, ws, _id) = workspace();
    let x = expand(&ws, "SELECT clean_phone(Phone) FROM contacts");
    assert!(x.contains("CAST('+' AS VARCHAR)"), "the default prefix: {x}");
    let named = expand(&ws, "SELECT clean_phone(prefix := '00', phone := Phone) FROM contacts");
    assert!(named.contains("CAST('00' AS VARCHAR)"), "{named}");

    let e = refused(&ws, "SELECT clean_phone(Phone, '+', 3) FROM contacts");
    assert!(e.contains("clean_phone takes 1 to 2 arguments") && e.contains("phone: column<text>"), "{e}");
    let e = refused(&ws, "SELECT weighted_average(price) FROM contacts");
    assert!(e.contains("weighted_average takes 2 arguments"), "{e}");
    let e = refused(&ws, "SELECT clean_phone(phone := Phone, nope := 1) FROM contacts");
    assert!(e.contains("no parameter nope"), "{e}");
    let e = refused(&ws, "SELECT clean_phone(Phone, phone := Phone) FROM contacts");
    assert!(e.contains("given twice"), "{e}");
}

#[test]
fn what_sql_mode_cant_call_is_refused_by_name() {
    let (_d, ws, _id) = workspace();
    let e = refused(&ws, "SELECT dedupe_by(region) FROM contacts");
    assert!(e.contains("dedupe_by") && e.contains("custom operation"), "{e}");
    let e = refused(&ws, "SELECT * FROM top_n_per_group(contacts)");
    assert!(e.contains("top_n_per_group is a custom operation"), "{e}");

    // A function with a columns parameter.
    let mut f = samples().into_iter().find(|d| d.name == "clean_phone").unwrap();
    f.name = "concat_cols".into();
    f.params = vec![parvu_core::custom::Param { name: "cols".into(), kind: ParamKind::Columns, types: vec!["text".into()], ..Default::default() }];
    f.body.source = "concat_ws('-', cols)".into();
    f.examples = vec![serde_json::from_value(serde_json::json!({
        "input": {"columns": [{"name": "cols__1", "dtype": "VARCHAR"}, {"name": "cols__2", "dtype": "VARCHAR"}], "rows": [["a", "b"]]},
        "args": {"cols": ["cols__1", "cols__2"]},
        "expected": {"columns": [{"name": "result"}], "rows": [["a-b"]]}
    }))
    .unwrap()];
    let mut ws = ws;
    ws.set_custom(Arc::new(Library::new([checked(f)])));
    let e = refused(&ws, "SELECT concat_cols(Phone, region) FROM contacts");
    assert!(e.contains("concat_cols takes several columns (cols)"), "{e}");

    // A draft can't be called.
    let mut draft = checked(samples().into_iter().find(|d| d.name == "clean_phone").unwrap());
    draft.status = parvu_core::custom::Status::Draft;
    ws.set_custom(Arc::new(Library::new([draft])));
    assert!(refused(&ws, "SELECT clean_phone(Phone) FROM contacts").contains("needs attention"));
}

#[test]
fn names_in_strings_and_quoted_columns_stay() {
    let (_d, ws, _id) = workspace();
    for sql in [
        "SELECT 'clean_phone(Phone)' AS s FROM contacts",
        "SELECT \"clean_phone\" FROM (SELECT 1 AS clean_phone)",
        "SELECT Phone AS clean_phone FROM contacts -- clean_phone(x)",
        "select  *\n  from contacts   where region = 'north'",
    ] {
        assert_eq!(expand(&ws, sql), sql, "byte for byte");
    }
    let x = expand(&ws, "SELECT clean_phone(Phone) AS p, 'clean_phone(x)' AS s, \"clean_phone\" FROM (SELECT Phone, 1 AS clean_phone FROM contacts)");
    assert!(x.contains("'clean_phone(x)'") && x.contains("clean_phone FROM"), "{x}");
    assert_eq!(x.matches("clean_phone(").count(), 1, "only the string keeps the call: {x}");
}

#[test]
fn ctes_and_subqueries() {
    let (_d, mut ws, id) = workspace();
    ws.run_sql(
        id,
        "WITH c AS (SELECT clean_phone(Phone) AS p, region FROM contacts) \
         SELECT region, (SELECT max(clean_phone(Phone, '#')) FROM contacts) AS m, count(p) AS n \
         FROM c WHERE p IN (SELECT clean_phone(Phone) FROM contacts) GROUP BY region ORDER BY region",
        None,
    )
    .unwrap();
    assert_eq!(rows(&ws, id), [["north", "#5550102", "1"], ["south", "#5550102", "1"]]);
}

#[test]
fn type_errors_name_the_function() {
    let (_d, mut ws, id) = workspace();
    let e = ws.run_sql(id, "SELECT clean_phone(price) FROM contacts", None).unwrap_err().to_string();
    assert!(e.starts_with("clean_phone:"), "{e}");
    assert!(!e.contains("LINE 1"), "{e}");
    assert!(ws.summary(id).unwrap().steps.is_empty());
}

#[test]
fn sql_into_a_new_table_expands_too() {
    let (_d, mut ws, id) = workspace();
    let derivation = Derivation::Sql { sql: "SELECT region, weighted_average(price, qty) AS wa FROM contacts GROUP BY ALL".into() };
    let plan = ws.plan_result(id, derivation).unwrap();
    materialize(ws.connection(), &plan).unwrap();
    let new = ws.add_result(plan).unwrap();
    assert_eq!(ws.summary(new).unwrap().total_rows, 2);
}

#[test]
fn read_only_queries_expand_before_the_guard() {
    let (_d, ws, _id) = workspace();
    let ro = ws.read_session().unwrap();
    let q = ro.query("SELECT region, weighted_average(price, qty) AS wa FROM contacts GROUP BY region ORDER BY 1", 10, None).unwrap();
    assert_eq!(q.rows[0], [Some("north".to_string()), Some("15.0".to_string())]);
    // A file function inside a custom argument is still refused.
    let e = ro
        .query("SELECT clean_phone((SELECT content FROM read_text('/etc/passwd'))) FROM contacts", 10, None)
        .unwrap_err()
        .to_string();
    assert!(e.contains("read_text"), "{e}");
    let e = ro.query("SELECT clean_phone(price) FROM contacts", 10, None).unwrap_err().to_string();
    assert!(e.starts_with("clean_phone:"), "{e}");
}

#[test]
fn a_duckdb_function_of_the_same_name_wins() {
    let mut f = checked(samples().into_iter().find(|d| d.name == "title_case").unwrap());
    f.name = "upper".into();
    let conn = duckdb::Connection::open_in_memory().unwrap();
    let lib = Library::new([f]);
    let sql = "SELECT upper('a')";
    assert_eq!(sqlmode::expand(&conn, sql, &lib).unwrap().sql, sql);
}

