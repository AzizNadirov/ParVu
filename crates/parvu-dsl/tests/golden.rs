//! Golden tests against `tests/golden/dsl.json`, which records the output of
//! the original Python implementation for 306 expressions. Entries in the
//! allowlist below are cases where the Python output was wrong.

use std::collections::HashMap;

use parvu_dsl::{
    Catalog, FunctionDef, FunctionRegistry, LogicalType, ParamDef, compile_replace, preview,
    strip_table_qualifier, translate,
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
    run_golden(&FunctionRegistry::default());
}

/// The built-in registry plus custom functions, as the app builds it for a
/// window with a custom library.
fn custom_registry() -> FunctionRegistry {
    let mut reg = FunctionRegistry::default();
    let typed = |name: &str, t: LogicalType| {
        let mut p = ParamDef::new(name, t);
        p.accepts = Some(vec![t]);
        p
    };
    reg.register(FunctionDef::custom(
        "clean_phone",
        vec![typed("phone", LogicalType::Text), ParamDef::with_default("prefix", LogicalType::Text, "CAST('+' AS VARCHAR)")],
        LogicalType::Text,
        "Keep the digits of a phone number, with a prefix.",
        "CASE WHEN (length(regexp_replace(__pv_arg_0, '[^0-9]', '', 'g')) < 7) THEN NULL \
         ELSE (CAST(__pv_arg_1 AS VARCHAR) || regexp_replace(__pv_arg_0, '[^0-9]', '', 'g')) END",
    ));
    let mut avg = FunctionDef::custom(
        "weighted_average",
        vec![typed("x", LogicalType::Numeric), typed("w", LogicalType::Numeric)],
        LogicalType::Numeric,
        "Average of x weighted by w.",
        "(sum((__pv_arg_0 * __pv_arg_1)) / sum(__pv_arg_1))",
    );
    avg.aggregate = true;
    reg.register(avg);
    reg.register(FunctionDef::custom(
        "digits2",
        vec![typed("s", LogicalType::Text)],
        LogicalType::Text,
        "A regex quantifier is not a slot.",
        "regexp_extract(__pv_arg_0, '[0-9]{2}')",
    ));
    reg
}

/// Custom functions change nothing the 306 pinned expressions compile to.
#[test]
fn golden_expressions_with_custom_functions() {
    run_golden(&custom_registry());
}

/// New cases: custom functions in expressions and formulas.
#[test]
fn golden_custom_functions() {
    let doc = load();
    let cats = catalogs(&doc);
    let main = &cats["main"];
    let reg = custom_registry();
    let tr = |src: &str| match translate(src, main, &reg, "sales") {
        Ok(t) => t.sql,
        Err(e) => format!("error: {e}"),
    };
    let cases: &[(&str, &str)] = &[
        (
            "clean_phone(sales[name])",
            "SELECT (CASE WHEN (length(regexp_replace((sales.name), '[^0-9]', '', 'g')) < 7) THEN NULL \
             ELSE (CAST(CAST('+' AS VARCHAR) AS VARCHAR) || regexp_replace((sales.name), '[^0-9]', '', 'g')) END) FROM sales",
        ),
        (
            "sales[mobile] = clean_phone(sales[name], '00')",
            "SELECT \"id\", \"revenue\", \"qty\", \"price\", \"name\", \"note\", \"flag\", \"active\", \"d\", \"ts\", \"new col\", \
             \"select\", \"Mixed\", \"x\"\"y\", \"tags\", \"café\", \"TRUE\", \"NOT\", (CASE WHEN (length(regexp_replace((sales.name), \
             '[^0-9]', '', 'g')) < 7) THEN NULL ELSE (CAST(('00') AS VARCHAR) || regexp_replace((sales.name), '[^0-9]', '', 'g')) END) \
             AS \"mobile\" FROM sales",
        ),
        ("CLEAN_PHONE(sales[note])", "SELECT (CASE WHEN (length(regexp_replace((sales.note), '[^0-9]', '', 'g')) < 7) THEN NULL \
             ELSE (CAST(CAST('+' AS VARCHAR) AS VARCHAR) || regexp_replace((sales.note), '[^0-9]', '', 'g')) END) FROM sales"),
        (
            "weighted_average(sales[price], sales[qty])",
            "SELECT ((sum(((sales.price) * (sales.qty))) / sum((sales.qty)))) FROM sales",
        ),
        ("digits2(UPPER(sales[name]))", "SELECT (regexp_extract((UPPER(sales.name)), '[0-9]{2}')) FROM sales"),
        ("clean_phone(sales[qty])", "error: clean_phone: phone takes text, but this argument is a number"),
        ("weighted_average(sales[name], sales[qty])", "error: weighted_average: x takes a number, but this argument is text"),
        ("digits2(sales[name], 1)", "error: 'digits2' takes at most 1 argument, got 2"),
        ("clean_phone()", "error: 'clean_phone' requires at least 1 arguments, got 0"),
    ];
    let mut failures = Vec::new();
    for (src, want) in cases {
        let got = tr(src);
        if normalize(&got) != normalize(want) {
            failures.push(format!("{src:?}\n   want {want}\n   got  {got}"));
        }
    }
    // In a formula a summary function is a column total.
    let f = parvu_dsl::compile_formula("= [price] - weighted_average([price], [qty])", main, &reg, "sales").unwrap();
    assert_eq!(f.totals, ["((sum(((sales.price) * (sales.qty))) / sum((sales.qty))))"]);
    assert_eq!(f.sql, format!("sales.price - ({})", parvu_dsl::total_marker(0)));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn run_golden(registry: &FunctionRegistry) {
    let doc = load();
    let cats = catalogs(&doc);
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
            translate(source, catalog, registry, table),
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

        let got_preview = as_value(preview(source, catalog, registry), |s| json!({ "sql": s }));
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
    // Regenerating after adding functions on purpose:
    // GOLDEN_REGISTRY_OUT=/path cargo test --test golden golden_registry
    // writes the registry as computed, to splice into dsl.json.
    if let Ok(out) = std::env::var("GOLDEN_REGISTRY_OUT") {
        let mut by_type = serde_json::Map::new();
        for lt in LogicalType::ALL {
            by_type.insert(lt.name().to_string(), json!(reg.methods_for_type(lt)));
        }
        let mut targets = serde_json::Map::new();
        for m in reg.all_methods() {
            targets.insert(m.clone(), json!(reg.lookup_method(&m).map(|f| f.name.clone())));
        }
        let dump = json!({ "functions": functions, "methods": reg.all_methods(), "methods_for_type": by_type, "method_targets": targets });
        std::fs::write(out, serde_json::to_string(&dump).unwrap()).unwrap();
        return;
    }
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
