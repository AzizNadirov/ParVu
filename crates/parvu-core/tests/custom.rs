//! Custom functions and operations: validation, applying them to tabs,
//! undo, result tabs and Refresh (plans/CUSTOM_TOOLS_PLAN.md §11.1).

use std::collections::BTreeMap;
use std::sync::Arc;

use parvu_core::colfn::Output;
use parvu_core::custom::{CustomArg, Definition, Library, Report, ValidateEnv, samples, validate};
use parvu_core::ops::Operation;
use parvu_core::workspace::{Derivation, materialize};
use parvu_core::{TabId, Workspace};

fn env() -> ValidateEnv<'static> {
    ValidateEnv { name_taken: &|_| None, app_version: "test" }
}

fn checked(mut d: Definition) -> Definition {
    let r = validate(&d, &env()).unwrap();
    assert!(r.passed(), "{}: {:#?}", d.name, r.checks);
    d.compiled = r.compiled;
    d
}

fn sample(name: &str) -> Definition {
    samples().into_iter().find(|d| d.name == name).unwrap()
}

fn report(d: &Definition) -> Report {
    validate(d, &env()).unwrap()
}

fn first_failure(d: &Definition) -> String {
    let r = report(d);
    assert!(!r.passed(), "{} should fail", d.name);
    r.first_failure().unwrap().message.key.clone()
}

#[test]
fn every_sample_validates() {
    let all = samples();
    assert_eq!(all.len(), 7);
    for d in all {
        assert!(d.sample && d.author == "ParVu");
        let r = report(&d);
        assert!(r.passed(), "{}: {:#?}\n{:#?}", d.name, r.checks, r.examples);
        assert!(r.examples.iter().all(|e| e.passed));
    }
}

#[test]
fn crafted_bodies_fail_their_check() {
    let mut d = sample("clean_phone");
    d.body.source = "phone || (SELECT content FROM read_text('/etc/passwd'))".into();
    assert_eq!(first_failure(&d), "custom.check.safety");
    d.body.source = "phone || (SELECT 1)".into();
    assert_eq!(first_failure(&d), "custom.check.function_subquery");
    d.body.source = "phone || random()::VARCHAR".into();
    assert_eq!(first_failure(&d), "custom.check.volatile");
    d.body.source = "string_agg(phone, ',')".into();
    assert_eq!(first_failure(&d), "custom.check.row_aggregates");
    d.body.source = "length(phone)".into();
    assert_eq!(first_failure(&d), "custom.check.result_type");

    let mut op = sample("top_n_per_group");
    op.body.source = "SELECT * FROM read_csv('/etc/passwd')".into();
    assert_eq!(first_failure(&op), "custom.check.safety");
    op.body.source = "SELECT * FROM t JOIN sales USING (grp)".into();
    assert_eq!(first_failure(&op), "custom.check.safety");
    op.body.source = "SELECT * FROM t QUALIFY row_number() OVER (PARTITION BY grp ORDER BY score DESC) <= n + 1".into();
    assert_eq!(first_failure(&op), "custom.check.example_differs");

    let mut s = sample("weighted_average");
    s.body.source = "x * w".into();
    assert_eq!(first_failure(&s), "custom.check.summary_not_aggregate");
}

#[test]
fn examples_report_the_actual_output() {
    let mut d = sample("clean_phone");
    d.examples[0].expected.rows[0][0] = Some("wrong".into());
    let r = report(&d);
    let out = &r.examples[0];
    assert!(!out.passed);
    assert_eq!(out.mismatches, vec![[0, 0]]);
    assert_eq!(out.actual.as_ref().unwrap().rows[0][0].as_deref(), Some("+15550102030"));
}

fn workspace(csv: &str, defs: Vec<Definition>) -> (tempfile::TempDir, Workspace, TabId) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("sales.csv");
    std::fs::write(&path, csv).unwrap();
    let mut ws = Workspace::new(100, None).unwrap();
    ws.set_custom(Arc::new(Library::new(defs.into_iter().map(checked))));
    let id = ws.open(&path).unwrap();
    (dir, ws, id)
}

fn args(pairs: &[(&str, CustomArg)]) -> BTreeMap<String, CustomArg> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

fn col(c: &str) -> CustomArg {
    CustomArg::Column(c.into())
}

fn column_values(ws: &Workspace, id: TabId, name: &str) -> Vec<Option<String>> {
    let page = ws.page(id, 1).unwrap();
    let i = page.columns.iter().position(|c| c.name == name).unwrap();
    page.rows.iter().map(|r| r[i].clone()).collect()
}

#[test]
fn row_function_in_place_and_new_column() {
    let (_d, mut ws, id) = workspace("Phone,qty\n+1 (555) 010-2030,1\nn/a,2\n", vec![sample("clean_phone")]);
    let op = Operation::CustomFunction {
        name: "clean_phone".into(),
        args: args(&[("phone", col("Phone"))]),
        output: Output::New { name: String::new() },
        filter: None,
    };
    let label = ws.apply(id, &op).unwrap();
    assert_eq!(label.key, "step.custom");
    assert_eq!(label.args["args"], "Phone");
    let names: Vec<_> = ws.columns(id).unwrap().into_iter().map(|c| c.name).collect();
    assert_eq!(names, ["Phone", "Phone_clean_phone", "qty"]);
    assert_eq!(column_values(&ws, id, "Phone_clean_phone"), [Some("+15550102030".into()), None]);
    ws.undo(id).unwrap();
    assert_eq!(ws.columns(id).unwrap().len(), 2);

    let op = Operation::CustomFunction {
        name: "clean_phone".into(),
        args: args(&[("phone", col("Phone")), ("prefix", CustomArg::Value("00".into()))]),
        output: Output::InPlace,
        filter: None,
    };
    ws.apply(id, &op).unwrap();
    assert_eq!(column_values(&ws, id, "Phone"), [Some("0015550102030".into()), None]);
}

#[test]
fn use_time_checks() {
    let (_d, ws, id) = workspace("Phone,qty\nx,1\n", vec![sample("clean_phone"), sample("weighted_average")]);
    let build = |op: Operation| ws.build_op(id, &op).map(|_| ()).unwrap_err().to_string();
    let f = |a: BTreeMap<String, CustomArg>| Operation::CustomFunction { name: "clean_phone".into(), args: a, output: Output::InPlace, filter: None };
    assert!(build(f(args(&[("phone", col("qty"))]))).contains("expected text"));
    assert!(build(f(args(&[("phone", col("nope"))]))).contains("nope"));
    assert!(build(f(args(&[]))).contains("choose a column"));
    assert!(build(f(args(&[("phone", col("Phone")), ("other", col("Phone"))]))).contains("no parameter other"));
    let missing = Operation::CustomFunction { name: "gone".into(), args: args(&[]), output: Output::InPlace, filter: None };
    assert!(build(missing).contains("no longer exists"));
    let summary = Operation::CustomFunction {
        name: "weighted_average".into(),
        args: args(&[("x", col("qty")), ("w", col("qty"))]),
        output: Output::InPlace,
        filter: None,
    };
    assert!(build(summary).contains("one value"));
}

#[test]
fn summary_value_runs_over_the_whole_table() {
    let (_d, ws, id) = workspace("x,w\n10,1\n20,1\n30,2\n", vec![sample("weighted_average")]);
    let (items, source, alias) = ws.custom_value_input(id, "weighted_average", &args(&[("x", col("x")), ("w", col("w"))])).unwrap();
    let v = parvu_core::sql::query_whole_table(ws.connection(), &items, &source, &alias, |r| r.get::<_, Option<String>>(0)).unwrap();
    assert_eq!(v.as_deref(), Some("22.5"));
}

#[test]
fn operation_as_a_step_and_undo() {
    let (_d, mut ws, id) = workspace("region,revenue\neast,10\neast,30\neast,20\nwest,5\n", vec![sample("top_n_per_group")]);
    let op = Operation::CustomOperation {
        name: "top_n_per_group".into(),
        args: args(&[("grp", col("region")), ("score", col("revenue")), ("n", CustomArg::Value("1".into()))]),
        new_table: false,
    };
    assert!(!op.is_table_op());
    ws.apply(id, &op).unwrap();
    let mut got = column_values(&ws, id, "revenue");
    got.sort();
    assert_eq!(got, [Some("30".into()), Some("5".into())]);
    ws.undo(id).unwrap();
    assert_eq!(ws.summary(id).unwrap().total_rows, 4);
    // Refused: the definition says it changes this table.
    let wrong = Operation::CustomOperation { name: "top_n_per_group".into(), args: BTreeMap::new(), new_table: true };
    assert!(ws.build_op(id, &wrong).is_err());
    // A value outside its bounds.
    let low = Operation::CustomOperation {
        name: "top_n_per_group".into(),
        args: args(&[("grp", col("region")), ("score", col("revenue")), ("n", CustomArg::Value("0".into()))]),
        new_table: false,
    };
    assert!(ws.build_op(id, &low).unwrap_err().to_string().contains("outside"));
}

#[test]
fn operation_into_a_new_table_and_refresh() {
    let (_d, mut ws, id) = workspace("id,amount\n1,1\n2,2\n3,3\n4,4\n5,100\n", vec![sample("outliers_iqr")]);
    let op = Operation::CustomOperation { name: "outliers_iqr".into(), args: args(&[("x", col("amount"))]), new_table: true };
    assert!(op.is_table_op());
    let plan = ws.plan_result(id, Derivation::Op { op: op.clone() }).unwrap();
    assert_eq!(plan.name, "sales_outliers_iqr");
    materialize(ws.connection(), &plan).unwrap();
    let new = ws.add_result(plan).unwrap();
    assert_eq!(column_values(&ws, new, "id"), [Some("5".into())]);

    // Refresh follows the current definition…
    let mut changed = sample("outliers_iqr");
    changed.params[2].default = Some("0".into());
    changed.examples[1].args.insert("k".into(), Some("1.5".into()));
    ws.set_custom(Arc::new(Library::new([checked(changed)])));
    let plan = ws.refresh_plan(new).unwrap();
    materialize(ws.connection(), &plan).unwrap();
    ws.replace_result(new, plan).unwrap();
    assert_eq!(ws.summary(new).unwrap().total_rows, 2, "k = 0: everything outside [Q1, Q3]");

    // …and fails with its name once it is deleted.
    ws.set_custom(Arc::new(Library::default()));
    let err = ws.refresh_plan(new).unwrap_err().to_string();
    assert!(err.contains("outliers_iqr") && err.contains("no longer exists"), "{err}");
    assert_eq!(ws.summary(new).unwrap().total_rows, 2, "the tab keeps its rows");
}

#[test]
fn running_total_adds_a_column() {
    let (_d, mut ws, id) = workspace("day,sales\n2024-01-02,5\n2024-01-01,10\n", vec![sample("running_total")]);
    let op = Operation::CustomOperation {
        name: "running_total".into(),
        args: args(&[("amount", col("sales")), ("sorted_by", col("day"))]),
        new_table: false,
    };
    ws.apply(id, &op).unwrap();
    assert_eq!(column_values(&ws, id, "running_total"), [Some("10".into()), Some("15".into())]);
}

#[test]
fn steps_keep_their_sql_when_the_definition_goes() {
    let (_d, mut ws, id) = workspace("x\n1\n3\n", vec![sample("percent_of_total")]);
    let op = Operation::CustomFunction { name: "percent_of_total".into(), args: args(&[("x", col("x"))]), output: Output::InPlace, filter: None };
    ws.apply(id, &op).unwrap();
    ws.set_custom(Arc::new(Library::default()));
    assert_eq!(column_values(&ws, id, "x"), [Some("25.0".into()), Some("75.0".into())]);
    ws.undo(id).unwrap();
    assert_eq!(column_values(&ws, id, "x"), [Some("1".into()), Some("3".into())]);
}

#[test]
fn custom_operations_deserialize_from_ui_json() {
    let op: Operation = serde_json::from_str(
        r#"{"op":"custom_function","name":"clean_phone","args":{"phone":{"column":"Phone"},"prefix":{"value":"+"}},"output":{"to":"new","name":"Phone_clean"}}"#,
    )
    .unwrap();
    assert!(matches!(op, Operation::CustomFunction { .. }));
    let op: Operation = serde_json::from_str(r#"{"op":"custom_operation","name":"top_n_per_group","args":{"n":{"value":"3"}}}"#).unwrap();
    assert!(matches!(op, Operation::CustomOperation { new_table: false, .. }));
}
