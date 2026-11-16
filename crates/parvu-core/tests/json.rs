//! JSON and nested columns: keys, expanding, paths, lists to rows.

use parvu_core::Workspace;
use parvu_core::colfn::{ColumnFn, JsonKey, Output};
use parvu_core::ops::Operation;
use tempfile::TempDir;

/// A CSV whose `payload` column is JSON text (typed JSON on open).
fn open_csv() -> (TempDir, Workspace, u64) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("events.csv");
    std::fs::write(
        &path,
        "id,payload,tags\n\
         1,\"{\"\"user\"\": \"\"ali\"\", \"\"age\"\": 31, \"\"addr\"\": {\"\"city\"\": \"\"Baku\"\"}}\",\"[\"\"a\"\", \"\"b\"\"]\"\n\
         2,\"{\"\"user\"\": \"\"vera\"\", \"\"id\"\": 9}\",\"[]\"\n\
         3,,\"[\"\"c\"\"]\"\n",
    )
    .unwrap();
    let mut ws = Workspace::new(100, None).unwrap();
    let id = ws.open(&path).unwrap();
    (dir, ws, id)
}

fn col(ws: &Workspace, id: u64, name: &str) -> Vec<Option<String>> {
    let page = ws.page(id, 1).unwrap();
    let i = page.columns.iter().position(|c| c.name == name).unwrap_or_else(|| panic!("no column {name}"));
    page.rows.into_iter().map(|r| r[i].clone()).collect()
}

fn names(ws: &Workspace, id: u64) -> Vec<String> {
    ws.columns(id).unwrap().into_iter().map(|c| c.name).collect()
}

#[test]
fn keys_are_sampled_and_expanded_with_types() {
    let (_d, mut ws, id) = open_csv();
    assert_eq!(ws.columns(id).unwrap()[1].dtype, "JSON");
    let keys = ws.json_keys(id, "payload", Some(2000)).unwrap();
    let find = |n: &str| keys.iter().find(|k| k.name == n).unwrap().dtype.clone();
    assert_eq!((find("user"), find("age"), find("addr"), find("id")), ("VARCHAR".into(), "BIGINT".into(), "JSON".into(), "BIGINT".into()));

    ws.apply(id, &Operation::ExpandJson { column: "payload".into(), keys: keys.clone(), keep_original: false }).unwrap();
    let cols: Vec<(String, String)> = ws.columns(id).unwrap().into_iter().map(|c| (c.name, c.dtype)).collect();
    // `id` already exists, so the key becomes payload.id; the others keep their names.
    assert!(cols.contains(&("payload.id".into(), "BIGINT".into())), "{cols:?}");
    assert!(cols.contains(&("age".into(), "BIGINT".into())));
    assert_eq!(names(&ws, id)[0], "id");
    assert_eq!(*names(&ws, id).last().unwrap(), "tags", "new columns take the JSON column's place");
    assert_eq!(col(&ws, id, "user"), vec![Some("ali".into()), Some("vera".into()), None]);
    assert_eq!(col(&ws, id, "age"), vec![Some("31".into()), None, None], "missing key: empty");
    assert_eq!(col(&ws, id, "addr")[0].as_deref(), Some("{\"city\":\"Baku\"}"));
    ws.undo(id).unwrap();

    let bad = JsonKey { name: "x".into(), dtype: "INTEGER); DROP TABLE t; --".into() };
    assert!(ws.apply(id, &Operation::ExpandJson { column: "payload".into(), keys: vec![bad], keep_original: true }).is_err());
}

#[test]
fn paths_lengths_and_rows() {
    let (_d, mut ws, id) = open_csv();
    let path = |p: &str, text| ColumnFn::JsonPath { path: p.into(), text };
    ws.apply(id, &Operation::ColumnTransform { column: "payload".into(), func: path("$.addr.city", true), output: Output::New { name: "city".into() } })
        .unwrap();
    assert_eq!(col(&ws, id, "city"), vec![Some("Baku".into()), None, None]);
    let bad = Operation::ColumnTransform { column: "payload".into(), func: path("$..[", false), output: Output::InPlace };
    assert!(ws.apply(id, &bad).is_err(), "a bad path is refused when applied");

    ws.apply(id, &Operation::ColumnTransform { column: "tags".into(), func: ColumnFn::ListLength, output: Output::New { name: "n".into() } })
        .unwrap();
    assert_eq!(col(&ws, id, "n"), vec![Some("2".into()), Some("0".into()), Some("1".into())]);
    ws.apply(
        id,
        &Operation::ColumnTransform { column: "tags".into(), func: ColumnFn::JoinList { separator: "|".into() }, output: Output::New { name: "joined".into() } },
    )
    .unwrap();
    assert_eq!(col(&ws, id, "joined"), vec![Some("a|b".into()), Some("".into()), Some("c".into())]);

    ws.apply(id, &Operation::ToRows { column: "tags".into() }).unwrap();
    assert_eq!(ws.summary(id).unwrap().total_rows, 4, "a, b, the empty list kept, c");
    assert_eq!(col(&ws, id, "tags")[1].as_deref(), Some("\"b\""));
    assert_eq!(col(&ws, id, "tags")[2], None);
}

#[test]
fn structs_and_lists_from_a_json_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("people.json");
    std::fs::write(
        &path,
        "{\"name\": \"ali\", \"addr\": {\"city\": \"Baku\", \"zip\": \"AZ1000\"}, \"scores\": [1, 2, 3]}\n\
         {\"name\": \"vera\", \"addr\": {\"city\": \"Gence\", \"zip\": null}, \"scores\": []}\n",
    )
    .unwrap();
    let mut ws = Workspace::new(100, None).unwrap();
    let id = ws.open(&path).unwrap();
    let types: Vec<String> = ws.columns(id).unwrap().into_iter().map(|c| c.dtype).collect();
    assert!(types[1].starts_with("STRUCT"), "{types:?}");
    assert!(types[2].ends_with("[]"), "{types:?}");

    ws.apply(id, &Operation::ExpandStruct { column: "addr".into() }).unwrap();
    assert_eq!(names(&ws, id), ["name", "city", "zip", "scores"]);
    assert_eq!(col(&ws, id, "city"), vec![Some("Baku".into()), Some("Gence".into())]);

    ws.apply(id, &Operation::ColumnTransform { column: "scores".into(), func: ColumnFn::ListLength, output: Output::New { name: "n".into() } })
        .unwrap();
    assert_eq!(col(&ws, id, "n"), vec![Some("3".into()), Some("0".into())]);
    ws.apply(id, &Operation::ToRows { column: "scores".into() }).unwrap();
    assert_eq!(ws.summary(id).unwrap().total_rows, 4, "1, 2, 3, and vera's empty list");
    assert!(ws.apply(id, &Operation::ExpandStruct { column: "name".into() }).is_err());
}
