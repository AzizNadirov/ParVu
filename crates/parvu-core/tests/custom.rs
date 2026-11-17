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
    ValidateEnv { name_taken: &|_| None, app_version: "test", expression_sql: None }
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
    assert_eq!(all.len(), 9);
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
    changed.examples[1].args.insert("k".into(), Some("1.5".to_string()).into());
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

// ---------------------------------------------------------------- columns parameters

fn cols(list: &[&str]) -> CustomArg {
    CustomArg::Columns(list.iter().map(|c| c.to_string()).collect())
}

#[test]
fn columns_parameters_apply_as_lists() {
    let (_d, mut ws, id) = workspace(
        "store,item,version,price\nA,tea,1,2.5\nA,tea,2,2.75\nA,milk,1,1.2\nB,tea,3,2.6\n",
        vec![sample("dedupe_by")],
    );
    let op = |keys: CustomArg| Operation::CustomOperation {
        name: "dedupe_by".into(),
        args: args(&[("keys", keys), ("latest", col("version"))]),
        new_table: false,
    };
    let label = ws.apply(id, &op(cols(&["store", "item"]))).unwrap();
    assert_eq!(label.args["args"], "[store, item], version");
    assert_eq!(ws.summary(id).unwrap().total_rows, 3);
    let mut prices = column_values(&ws, id, "price");
    prices.sort();
    assert_eq!(prices, [Some("1.2".into()), Some("2.6".into()), Some("2.75".into())]);
    ws.undo(id).unwrap();
    // One key: one row per store.
    ws.apply(id, &op(cols(&["store"]))).unwrap();
    assert_eq!(ws.summary(id).unwrap().total_rows, 2);
    ws.undo(id).unwrap();
    // Use-time checks: count, duplicates, unknown columns.
    let err = |a: CustomArg| ws.build_op(id, &op(a)).unwrap_err().to_string();
    assert!(err(cols(&[])).contains("choose at least 1 columns"), "{}", err(cols(&[])));
    assert!(err(cols(&["store", "store"])).contains("chosen twice"));
    assert!(err(cols(&["store", "nope"])).contains("nope"));
}

#[test]
fn columns_parameters_only_where_sql_takes_a_list() {
    let mut d = sample("dedupe_by");
    let fails = |d: &Definition| first_failure(d);
    for body in [
        "SELECT * FROM t WHERE keys > 1",
        "SELECT * FROM t ORDER BY keys DESC",
        "SELECT keys AS k FROM t",
        "SELECT t.keys FROM t",
        "SELECT keys + 1 AS k FROM t",
    ] {
        d.body.source = body.into();
        assert_eq!(fails(&d), "custom.check.columns_position", "{body}");
    }
    // Allowed: select list, GROUP BY, function arguments, ORDER BY without a
    // direction, DISTINCT ON, COALESCE.
    let mut g = d.clone();
    g.params[2].kind = parvu_core::custom::ParamKind::Column;
    for body in [
        "SELECT DISTINCT ON (keys) * FROM t ORDER BY keys, latest DESC",
        "SELECT keys, count(*) AS n, max(latest) AS m FROM t GROUP BY keys",
        "SELECT *, concat_ws('/', keys) AS k, coalesce(keys) AS c FROM t QUALIFY row_number() OVER (PARTITION BY keys ORDER BY latest) = 1",
    ] {
        g.body.source = body.into();
        let r = report(&g);
        assert!(r.checks.iter().all(|c| c.message.key != "custom.check.columns_position"), "{body}: {:#?}", r.checks);
        assert!(r.checks.iter().any(|c| c.id == "types" && c.status == parvu_core::custom::CheckStatus::Pass), "{body}: {:#?}", r.checks);
    }
    // A count the parameter can't have.
    let mut bad = sample("dedupe_by");
    bad.params[1].min = Some(3.0);
    bad.params[1].max = Some(2.0);
    assert_eq!(first_failure(&bad), "custom.check.columns_count");
}

#[test]
fn a_function_takes_a_list_of_columns() {
    let mut d = sample("title_case");
    d.name = "join_text".into();
    d.params = vec![
        parvu_core::custom::Param { name: "parts".into(), kind: parvu_core::custom::ParamKind::Columns, types: vec!["text".into()], min: Some(2.0), ..Default::default() },
    ];
    d.body.source = "concat_ws(' ', parts)".into();
    d.examples = vec![parvu_core::custom::Example {
        input: parvu_core::custom::ExampleTable {
            columns: vec![
                parvu_core::custom::ExampleColumn { name: "a".into(), dtype: "VARCHAR".into() },
                parvu_core::custom::ExampleColumn { name: "b".into(), dtype: "VARCHAR".into() },
            ],
            rows: vec![vec![Some("x".into()), Some("y".into())], vec![None, Some("z".into())]],
        },
        args: [("parts".to_string(), parvu_core::custom::ExampleArg::Columns(vec!["a".into(), "b".into()]))].into(),
        expected: parvu_core::custom::ExampleTable {
            columns: vec![parvu_core::custom::ExampleColumn { name: "result".into(), dtype: String::new() }],
            rows: vec![vec![Some("x y".into())], vec![Some("z".into())]],
        },
        ..Default::default()
    }];
    assert!(d.signature().contains("parts: columns<text> × 2+"), "{}", d.signature());
    let (_d, mut ws, id) = workspace("first,last,n\nAnn,Lee,1\nBob,,2\n", vec![d]);
    let op = Operation::CustomFunction {
        name: "join_text".into(),
        args: args(&[("parts", cols(&["first", "last"]))]),
        output: Output::New { name: "full".into() },
        filter: None,
    };
    ws.apply(id, &op).unwrap();
    let names: Vec<_> = ws.columns(id).unwrap().into_iter().map(|c| c.name).collect();
    assert_eq!(names, ["first", "full", "last", "n"], "after its first column");
    assert_eq!(column_values(&ws, id, "full"), [Some("Ann Lee".into()), Some("Bob".into())]);
    // Too few columns.
    let one = Operation::CustomFunction { name: "join_text".into(), args: args(&[("parts", cols(&["first"]))]), output: Output::InPlace, filter: None };
    assert!(ws.build_op(id, &one).unwrap_err().to_string().contains("at least 2"));
}

#[test]
fn example_args_read_lists_and_values() {
    let d = sample("dedupe_by");
    let json = serde_json::to_value(&d.examples[1].args).unwrap();
    assert_eq!(json, serde_json::json!({ "keys": ["store", "item"], "latest": "version" }));
}

// ---------------------------------------------------------------- custom aggregates in Group By

#[test]
fn summary_functions_aggregate_in_group_by() {
    use parvu_core::reshape::{AggFunc, Aggregation, CustomAggregate};
    let (_d, ws, id) = workspace(
        "region,x\neast,1\neast,2\neast,3\neast,4\nwest,10\n",
        vec![sample("percentile_value"), sample("weighted_average")],
    );
    let custom = |name: &str, values: &[(&str, &str)]| Aggregation {
        column: "x".into(),
        func: AggFunc::Custom,
        alias: None,
        custom: Some(CustomAggregate { name: name.into(), values: values.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }),
    };
    let op = Operation::GroupBy { by: vec!["region".into()], aggregations: vec![custom("percentile_value", &[("p", "0.5")]), custom("percentile_value", &[])] };
    let built = ws.build_op(id, &op).unwrap();
    assert_eq!(built.label.args["aggregations"], "percentile_value(x), percentile_value(x)");
    let plan = ws.plan_result(id, Derivation::Op { op }).unwrap();
    materialize(ws.connection(), &plan).unwrap();
    let mut ws = ws;
    let new = ws.add_result(plan).unwrap();
    let names: Vec<_> = ws.columns(new).unwrap().into_iter().map(|c| c.name).collect();
    assert_eq!(names, ["region", "x_percentile_value", "x_percentile_value_2"]);
    assert_eq!(column_values(&ws, new, "x_percentile_value"), [Some("2.5".into()), Some("10.0".into())]);
    assert_eq!(column_values(&ws, new, "x_percentile_value_2"), [Some("3.7".into()), Some("10.0".into())], "the default p = 0.9");
    // Refused: two column parameters, a bad value, an unknown function.
    let err = |a: Aggregation| ws.build_op(id, &Operation::GroupBy { by: vec!["region".into()], aggregations: vec![a] }).unwrap_err().to_string();
    assert!(err(custom("weighted_average", &[])).contains("one column"));
    assert!(err(custom("percentile_value", &[("p", "2")])).contains("outside"));
    assert!(err(custom("gone", &[])).contains("no longer exists"));
    assert!(err(Aggregation { custom: None, ..custom("percentile_value", &[]) }).contains("Choose the custom function"));
}

// ---------------------------------------------------------------- steps → operation

#[test]
fn steps_become_an_operation_that_runs_on_another_file() {
    let (_d, mut ws, id) = workspace("region,amount,note\neast,10,a\nwest,5,b\neast,30,c\nwest,40,d\n", vec![]);
    ws.edit_cell(id, 0, "note", Some("A".into())).unwrap();
    ws.apply(id, &serde_json::from_value(serde_json::json!({
        "op": "filter", "conditions": [{ "column": "amount", "op": "gt", "value": "6" }], "combine": "and"
    })).unwrap()).unwrap();
    ws.apply(id, &Operation::AddColumn { name: "double".into(), expression: "amount * 2".into() }).unwrap();
    ws.apply(id, &Operation::Sort { column: "amount".into(), ascending: false }).unwrap();
    let capture = ws.steps_capture(id).unwrap();
    assert_eq!(capture.steps.len(), 4);

    // Fixed columns: they become required columns of the table parameter.
    let draft = parvu_core::custom::record::draft(&capture, &[]).unwrap();
    assert!(!draft.steps[0].included, "cell edits are under the steps");
    assert_eq!(draft.steps[0].note.as_ref().unwrap().key, "custom.steps.cell_edits");
    assert!(draft.steps[1..].iter().all(|s| s.included), "{:#?}", draft.steps);
    let def = draft.definition.unwrap();
    assert!(def.body.source.starts_with("WITH step_1 AS"), "{}", def.body.source);
    assert!(def.examples[0].ordered, "the steps sort");
    let required: Vec<_> = def.params[0].columns.iter().map(|c| c.name.as_str()).collect();
    assert!(required.contains(&"amount"), "{required:?}");
    let r = report(&Definition { name: "my_steps".into(), ..def.clone() });
    assert!(r.passed(), "{}\n{:#?}", def.body.source, r.checks);
    assert_eq!(r.examples[0].actual.as_ref().unwrap().rows.len(), 3);

    // `amount` as a parameter: applied to another file with another name.
    let draft = parvu_core::custom::record::draft(&capture, &["amount".to_string()]).unwrap();
    let amount = draft.columns.iter().find(|c| c.name == "amount").unwrap();
    assert_eq!(amount.param.as_deref(), Some("amount"));
    let mut def = draft.definition.unwrap();
    def.name = "big_doubled".into();
    def.status = parvu_core::custom::Status::Ok;
    assert!(def.params.iter().any(|p| p.name == "amount" && p.kind == parvu_core::custom::ParamKind::Column));
    let dir = tempfile::TempDir::new().unwrap();
    let other = dir.path().join("other.csv");
    std::fs::write(&other, "price,region\n100,x\n1,y\n").unwrap();
    let mut ws2 = Workspace::new(100, None).unwrap();
    ws2.set_custom(Arc::new(Library::new([checked(def)])));
    let id2 = ws2.open(&other).unwrap();
    let op = Operation::CustomOperation { name: "big_doubled".into(), args: args(&[("amount", col("price"))]), new_table: false };
    ws2.apply(id2, &op).unwrap();
    assert_eq!(column_values(&ws2, id2, "double"), [Some("200".into())]);
}

#[test]
fn steps_that_cant_be_saved_are_explained() {
    let (_d, mut ws, id) = workspace("a,b\n1,2\n3,4\n", vec![]);
    let dir = tempfile::TempDir::new().unwrap();
    let other = dir.path().join("other.csv");
    std::fs::write(&other, "a\n1\n").unwrap();
    ws.open(&other).unwrap();
    ws.apply(id, &Operation::Sort { column: "a".into(), ascending: true }).unwrap();
    // SQL mode reads the table under the steps by name: it can be saved.
    ws.run_sql(id, "SELECT a, b, a + b AS c FROM sales", None).unwrap();
    // This one reads another tab: it and what follows are left out.
    ws.run_sql(id, "SELECT * FROM sales WHERE a IN (SELECT a FROM other)", None).unwrap();
    ws.apply(id, &Operation::Sort { column: "b".into(), ascending: true }).unwrap();
    let draft = parvu_core::custom::record::draft(&ws.steps_capture(id).unwrap(), &[]).unwrap();
    let keys: Vec<_> = draft.steps.iter().map(|s| (s.included, s.note.as_ref().map(|n| n.key.as_str()))).collect();
    assert_eq!(
        keys,
        [(true, None), (true, None), (false, Some("custom.steps.other_table")), (false, Some("custom.steps.after_stop"))]
    );
    assert_eq!(draft.steps[2].note.as_ref().unwrap().args["table"], "other");
    let def = draft.definition.unwrap();
    assert!(def.body.source.contains("FROM t"), "{}", def.body.source);
    let r = report(&Definition { name: "my_steps".into(), ..def });
    assert!(r.passed(), "{:#?}", r.checks);
}

#[test]
fn a_column_a_step_rewrites_stays_fixed() {
    let (_d, mut ws, id) = workspace("Phone,qty\n+1 555 0102 030,1\n", vec![sample("clean_phone")]);
    let op = Operation::CustomFunction { name: "clean_phone".into(), args: args(&[("phone", col("Phone"))]), output: Output::InPlace, filter: None };
    ws.apply(id, &op).unwrap();
    let draft = parvu_core::custom::record::draft(&ws.steps_capture(id).unwrap(), &["Phone".to_string()]).unwrap();
    let phone = draft.columns.iter().find(|c| c.name == "Phone").unwrap();
    assert!(phone.param.is_none());
    assert_eq!(phone.fixed.as_ref().unwrap().key, "custom.steps.column_written");
    let def = draft.definition.unwrap();
    assert!(!def.body.source.contains("__pv_"));
    let r = report(&Definition { name: "my_steps".into(), ..def });
    assert!(r.passed(), "{:#?}", r.checks);
}
