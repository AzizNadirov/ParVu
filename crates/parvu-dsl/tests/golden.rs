//! Golden tests: Rust output must equal the Python DSL output recorded by
//! `scripts/dsl_golden.py` in `tests/golden/dsl.json`.

use std::collections::HashMap;

use parvu_dsl::{
    Catalog, FunctionRegistry, LogicalType, compile_replace, preview, strip_table_qualifier,
    translate,
};
use serde_json::{Value, json};

/// Cases where the Rust crate deliberately differs from the Python DSL.
/// (source, Rust translate result: SQL text or `error:<kind>`, reason)
const ALLOWLIST: &[(&str, &str, &str)] = &[
    (
        "FALSE",
        "SELECT FALSE FROM sales",
        "Python builds exp.Boolean(this=\"FALSE\"), which sqlglot renders as TRUE",
    ),
    (
        "(sales[id] + sales[qty]) * 2",
        "SELECT (sales.id + sales.qty) * 2 FROM sales",
        "sqlglot drops the parentheses, changing the meaning",
    ),
    (
        "sales[id] - (sales[qty] - 1)",
        "SELECT sales.id - (sales.qty - 1) FROM sales",
        "sqlglot drops the parentheses, changing the meaning",
    ),
    (
        "sales[id] / (sales[qty] / 2)",
        "SELECT sales.id / (sales.qty / 2) FROM sales",
        "sqlglot drops the parentheses, changing the meaning",
    ),
    (
        "(sales[flag] OR sales[active]) AND sales[flag]",
        "SELECT (sales.flag OR sales.active) AND sales.flag FROM sales",
        "sqlglot drops the parentheses, changing the meaning",
    ),
    (
        "sales[id] > 1 == TRUE",
        "SELECT (sales.id > 1) = TRUE FROM sales",
        "Python emits chained comparisons, a DuckDB syntax error",
    ),
    (
        "-sales[id]",
        "SELECT -sales.id FROM sales",
        "Python resolver crashes (IndexError) on every unary operator",
    ),
    (
        "- 5",
        "SELECT -5 FROM sales",
        "Python resolver crashes (IndexError) on every unary operator",
    ),
    (
        "-(sales[id] + 1)",
        "SELECT -(sales.id + 1) FROM sales",
        "Python resolver crashes (IndexError) on every unary operator",
    ),
    (
        "NOT sales[flag]",
        "SELECT NOT sales.flag FROM sales",
        "Python resolver crashes (IndexError) on every unary operator",
    ),
    (
        "!sales[flag]",
        "SELECT NOT sales.flag FROM sales",
        "Python resolver crashes (IndexError) on every unary operator",
    ),
    (
        "NOT (sales[flag] AND sales[active])",
        "SELECT NOT (sales.flag AND sales.active) FROM sales",
        "Python resolver crashes (IndexError) on every unary operator",
    ),
    (
        "+sales[id]",
        "SELECT sales.id FROM sales",
        "Python resolver crashes (IndexError) on every unary operator",
    ),
    (
        "UPPER(drop_null(sales[note]))",
        "error:resolve",
        "Python splices a whole SELECT into a function call (invalid SQL)",
    ),
    (
        "sales[n] = drop_duplicates(sales[id])",
        "error:resolve",
        "Python splices a whole SELECT into the select list (invalid SQL)",
    ),
    (
        "drop_null(sales[note], 'x') + 1",
        "error:resolve",
        "Python appends '+ 1' to the WHERE clause, silently changing the filter",
    ),
];

fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn load() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/dsl.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("golden file")).expect("valid JSON")
}

fn catalogs(doc: &Value) -> HashMap<String, Catalog> {
    let mut out = HashMap::new();
    for (name, tables) in doc["catalogs"].as_object().unwrap() {
        let mut cat = Catalog::new();
        for t in tables.as_array().unwrap() {
            let cols = t["columns"].as_array().unwrap().iter().map(|c| {
                (
                    c[0].as_str().unwrap().to_string(),
                    c[1].as_str().unwrap().to_string(),
                )
            });
            cat.register_table(t["name"].as_str().unwrap(), cols);
        }
        out.insert(name.clone(), cat);
    }
    out
}

/// Render a result as comparable JSON: `{sql, kind}` / `{sql}` / `{error_kind}`.
fn as_value<T: serde::Serialize>(
    r: Result<T, parvu_dsl::DslError>,
    f: impl Fn(T) -> Value,
) -> Value {
    match r {
        Ok(v) => f(v),
        Err(e) => json!({ "error_kind": serde_json::to_value(&e).unwrap()["kind"] }),
    }
}

fn normalized(v: &Value) -> Value {
    let mut v = v.clone();
    if let Some(sql) = v.get("sql").and_then(Value::as_str) {
        v["sql"] = Value::String(normalize(sql));
    }
    v
}

#[test]
fn golden_expressions() {
    let doc = load();
    let cats = catalogs(&doc);
    let registry = FunctionRegistry::default();
    let allow: HashMap<&str, (&str, &str)> =
        ALLOWLIST.iter().map(|(s, e, r)| (*s, (*e, *r))).collect();

    let mut failures = Vec::new();
    let mut passed = 0;
    let mut allowlisted_seen = 0;
    let cases = doc["cases"].as_array().unwrap();
    for case in cases {
        let source = case["source"].as_str().unwrap();
        let table = case["table"].as_str().unwrap();
        let catalog = &cats[case["catalog"].as_str().unwrap()];

        let got = as_value(
            translate(source, catalog, &registry, table),
            |t| json!({ "sql": t.sql, "kind": t.kind }),
        );

        if let Some((expected, reason)) = allow.get(source) {
            allowlisted_seen += 1;
            let got_text = match got.get("sql") {
                Some(sql) => sql.as_str().unwrap().to_string(),
                None => format!("error:{}", got["error_kind"].as_str().unwrap()),
            };
            if normalize(&got_text) != normalize(expected) {
                failures.push(format!(
                    "[allowlisted: {reason}] {source:?}\n   want {expected}\n   got  {got_text}"
                ));
            } else {
                passed += 1;
            }
            continue;
        }

        let got_preview = as_value(preview(source, catalog, &registry), |s| json!({ "sql": s }));
        let mut ok = true;
        if normalized(&got) != normalized(&case["expected"]) {
            ok = false;
            failures.push(format!(
                "{source:?} (translate)\n   want {}\n   got  {}",
                case["expected"], got
            ));
        }
        if normalized(&got_preview) != normalized(&case["preview"]) {
            ok = false;
            failures.push(format!(
                "{source:?} (preview)\n   want {}\n   got  {}",
                case["preview"], got_preview
            ));
        }
        if ok {
            passed += 1;
        }
    }
    assert_eq!(
        allowlisted_seen,
        ALLOWLIST.len(),
        "every allowlist entry must match a golden case"
    );
    println!(
        "golden: {passed}/{} cases passed ({} allowlisted)",
        cases.len(),
        ALLOWLIST.len()
    );
    assert!(
        failures.is_empty(),
        "{} golden failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn golden_replace_dialog() {
    let doc = load();
    for case in doc["replace_cases"].as_array().unwrap() {
        let table = case["table"].as_str().unwrap();
        let sql = compile_replace(
            table,
            case["column"].as_str().unwrap(),
            case["pattern"].as_str().unwrap(),
            case["replacement"].as_str().unwrap(),
            case["case_sensitive"].as_bool().unwrap(),
            case["regex"].as_bool().unwrap(),
        );
        assert_eq!(sql, case["sql"].as_str().unwrap(), "{case}");
        assert_eq!(
            strip_table_qualifier(&sql, table),
            case["stripped"].as_str().unwrap(),
            "{case}"
        );
    }
}

#[test]
fn golden_registry() {
    let doc = load();
    let want = &doc["registry"];
    let reg = FunctionRegistry::default();

    let functions: Vec<Value> = reg
        .all_functions()
        .into_iter()
        .map(|f| {
            json!({
                "name": f.name,
                "params": f.params,
                "return_type": f.return_type,
                "description": f.description,
                "example": f.example,
                "sql_name": f.sql_name,
            })
        })
        .collect();
    assert_eq!(Value::Array(functions), want["functions"]);
    assert_eq!(json!(reg.all_methods()), want["methods"]);

    let mut by_type = serde_json::Map::new();
    for lt in LogicalType::ALL {
        by_type.insert(lt.name().to_string(), json!(reg.methods_for_type(lt)));
    }
    assert_eq!(Value::Object(by_type), want["methods_for_type"]);

    let mut targets = serde_json::Map::new();
    for m in reg.all_methods() {
        targets.insert(
            m.clone(),
            json!(reg.lookup_method(&m).map(|f| f.name.clone())),
        );
    }
    assert_eq!(Value::Object(targets), want["method_targets"]);
}
