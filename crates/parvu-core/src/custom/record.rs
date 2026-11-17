//! Applied steps → a custom operation ("Save steps as custom operation…",
//! plans/CUSTOM_TOOLS_PLAN.md §4.4).
//!
//! Each query step records the query it was built on (`source`) and its own
//! SQL, in which `source` appears verbatim (operations and expressions wrap
//! the previous result). The steps become a chain of CTEs: in a step's SQL,
//! `source` is replaced by `SELECT * FROM <the CTE of the step that made
//! it>`, the table under the steps being the table parameter `t`. SQL typed
//! in SQL mode reads the table under the steps by the tab's name, which
//! becomes `t`. A step that reads another tab, or one whose source isn't
//! known, ends the chain: it and the steps after it are left out, with the
//! reason. Cell edits, formulas and "Read as" choices change the table under
//! the steps; the operation starts from that table and leaves them out.
//!
//! Columns of the table the steps read become required columns of `t`, or
//! column parameters where the user chooses, by renaming their references.
//! A column a step writes (an alias, `* REPLACE`, `* RENAME`, `* EXCLUDE`)
//! can't become a parameter: later steps would read the step's column, not
//! the chosen one.

use std::collections::{BTreeSet, HashMap};

use duckdb::Connection;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::compile::{compile, function_stability, reserved_words};
use super::examples::run_example;
use super::signature::{family_name, is_valid_param_name};
use super::validate::{Issue, validation_connection};
use super::{
    Body, DefKind, Definition, Example, ExampleArg, ExampleColumn, ExampleTable, MAX_EXAMPLE_COLUMNS, Param, ParamKind,
    Placement, RequiredColumn, ResultKind, ResultSpec, Status,
};
use crate::error::Result;
use crate::ops::{Operation, StepLabel};
use crate::page::{Cell, ColumnMeta};
use crate::types::TypeFamily;

/// Rows of the table under the steps read for the example.
pub const SAMPLE_ROWS: usize = 50;
/// Rows the example starts with; more when the steps keep none of them.
const EXAMPLE_ROWS: usize = 10;

/// How a step was made (see [`crate::Workspace::steps_capture`]).
#[derive(Debug, Clone)]
pub enum CapturedKind {
    /// A query step: `source` is the query it was built on (`None`: SQL
    /// typed in SQL mode, reading the table under the steps by name).
    Query { source: Option<String>, sql: String, op: Option<Operation> },
    /// A query step with no record of how it was made.
    Unrecorded,
    CellEdits,
    Formulas,
    SourceReading,
}

#[derive(Debug, Clone)]
pub struct CapturedStep {
    pub label: StepLabel,
    pub kind: CapturedKind,
}

/// A tab's steps and the table under them.
#[derive(Debug, Clone)]
pub struct StepsCapture {
    /// The tab's name.
    pub table: String,
    /// `SELECT * FROM "<name>"`: the table under the steps.
    pub base_query: String,
    pub columns: Vec<ColumnMeta>,
    /// Its first [`SAMPLE_ROWS`] rows, as text.
    pub rows: Vec<Vec<Cell>>,
    pub steps: Vec<CapturedStep>,
    /// Formula columns of the table (they are part of it).
    pub formulas: Vec<String>,
}

/// One step and whether it is part of the operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepNote {
    pub label: StepLabel,
    pub included: bool,
    /// Why it is left out (or a note about it), as a translation key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<Issue>,
}

/// A column of the table under the steps that the steps read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnChoice {
    pub name: String,
    pub dtype: String,
    /// The parameter it became, if chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
    /// Why it must stay a fixed column, as a translation key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed: Option<Issue>,
}

/// What [`draft`] gives: the operation (unless no step can be part of it),
/// every step with its note, and the columns the steps read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepsDraft {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<Definition>,
    pub steps: Vec<StepNote>,
    pub columns: Vec<ColumnChoice>,
    /// Why the example has no expected output, if it couldn't run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

fn note(key: &str, args: &[(&str, &str)]) -> Option<Issue> {
    Some(StepLabel::new(key, args))
}

fn serialize(conn: &Connection, sql: &str) -> Option<Value> {
    let json: String = conn.query_row("SELECT json_serialize_sql(?)", [sql], |r| r.get(0)).ok()?;
    let tree: Value = serde_json::from_str(&json).ok()?;
    (tree["error"].as_bool() != Some(true) && tree["statements"].as_array().is_some_and(|s| s.len() == 1)).then_some(tree)
}

fn deserialize(conn: &Connection, tree: &Value) -> Option<String> {
    let text = serde_json::to_string(tree).ok()?;
    conn.query_row("SELECT json_deserialize_sql(?::JSON)", [text], |r| r.get(0)).ok()
}

/// CTE names defined anywhere in `v`, lower-cased.
fn ctes(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(map) => {
            if let Some(entries) = map.get("cte_map").and_then(|m| m["map"].as_array()) {
                out.extend(entries.iter().filter_map(|e| e["key"].as_str()).map(str::to_lowercase));
            }
            map.values().for_each(|c| ctes(c, out));
        }
        Value::Array(items) => items.iter().for_each(|c| ctes(c, out)),
        _ => {}
    }
}

/// Tables `v` reads by name (not its own CTEs), lower-cased, with a schema
/// or catalog marking a name as never known.
fn base_tables(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("BASE_TABLE") {
                let name = map.get("table_name").and_then(Value::as_str).unwrap_or("");
                let qualified = ["schema_name", "catalog_name"]
                    .iter()
                    .any(|k| map.get(*k).and_then(Value::as_str).is_some_and(|s| !s.is_empty()));
                out.insert(if qualified { format!("{name}.") } else { name.to_lowercase() });
            }
            map.values().for_each(|c| base_tables(c, out));
        }
        Value::Array(items) => items.iter().for_each(|c| base_tables(c, out)),
        _ => {}
    }
}

/// Rename the tables `rename` maps (lower-cased name → new name), keeping
/// the old name as the alias so `sales.price` still resolves.
fn rename_tables(v: &mut Value, rename: &HashMap<String, String>) {
    match v {
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("BASE_TABLE")
                && let Some(name) = map.get("table_name").and_then(Value::as_str).map(str::to_string)
                && let Some(to) = rename.get(&name.to_lowercase())
            {
                map.insert("table_name".into(), Value::String(to.clone()));
                map.insert("qualified_name".into(), serde_json::json!({ "path": [to] }));
                if map.get("alias").and_then(Value::as_str).unwrap_or("").is_empty() {
                    map.insert("alias".into(), Value::String(name));
                }
            }
            map.values_mut().for_each(|c| rename_tables(c, rename));
        }
        Value::Array(items) => items.iter_mut().for_each(|c| rename_tables(c, rename)),
        _ => {}
    }
}

/// Where a column reference names input column `col`: the index of the
/// part that is the column (`col`, `col.field`, `sales.col`), if any.
fn names_column(parts: &[String], col: &str, columns: &[String]) -> Option<usize> {
    let is = |s: &String| s.eq_ignore_ascii_case(col);
    match parts {
        [first, ..] if is(first) => Some(0),
        // A qualifier (`sales.col`): the first part isn't a column.
        [first, second, ..] if is(second) && !columns.iter().any(|c| c.eq_ignore_ascii_case(first)) => Some(1),
        _ => None,
    }
}

fn column_parts(map: &serde_json::Map<String, Value>) -> Vec<String> {
    map.get("column_names")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|n| n.as_str().unwrap_or("").to_string()).collect())
        .unwrap_or_default()
}

/// Names a star expression lists (`EXCLUDE`, `REPLACE`, `RENAME`).
fn star_names(map: &serde_json::Map<String, Value>) -> Vec<String> {
    let text = |v: &Value| v.as_str().map(str::to_string).or_else(|| v["column"].as_str().map(str::to_string));
    let mut out = Vec::new();
    if let Some(Value::Array(items)) = map.get("exclude_list") {
        out.extend(items.iter().filter_map(text));
    }
    if let Some(Value::Array(items)) = map.get("replace_list") {
        out.extend(items.iter().filter_map(|i| text(&i["key"])));
    }
    if let Some(Value::Array(items)) = map.get("rename_list") {
        out.extend(items.iter().filter_map(|i| text(&i["key"])));
        out.extend(items.iter().filter_map(|i| text(&i["value"])));
    }
    out
}

/// What the steps do with the input columns: those read, and those written
/// (an alias or a star list naming them), lower-cased.
fn column_use(v: &Value, columns: &[String], read: &mut BTreeSet<String>, written: &mut BTreeSet<String>) {
    match v {
        Value::Object(map) => {
            if map.get("class").and_then(Value::as_str) == Some("COLUMN_REF") {
                let parts = column_parts(map);
                for c in columns {
                    if names_column(&parts, c, columns).is_some() {
                        read.insert(c.to_lowercase());
                    }
                }
            }
            if let Some(alias) = map.get("alias").and_then(Value::as_str)
                && !alias.is_empty()
                && map.get("type").and_then(Value::as_str) != Some("BASE_TABLE")
                && map.get("type").and_then(Value::as_str) != Some("SUBQUERY")
            {
                written.insert(alias.to_lowercase());
            }
            if map.get("type").and_then(Value::as_str) == Some("STAR") {
                for n in star_names(map) {
                    read.insert(n.to_lowercase());
                    written.insert(n.to_lowercase());
                }
            }
            map.values().for_each(|c| column_use(c, columns, read, written));
        }
        Value::Array(items) => items.iter().for_each(|c| column_use(c, columns, read, written)),
        _ => {}
    }
}

/// Rename references to input columns (`col` → `params[col]`), dropping a
/// table qualifier: in an operation body a parameter is written bare.
fn rename_columns(v: &mut Value, params: &HashMap<String, String>, columns: &[String]) {
    match v {
        Value::Object(map) => {
            if map.get("class").and_then(Value::as_str) == Some("COLUMN_REF") {
                let parts = column_parts(map);
                for (col, param) in params {
                    if let Some(i) = names_column(&parts, col, columns) {
                        let mut next: Vec<Value> = vec![Value::String(param.clone())];
                        next.extend(parts[i + 1..].iter().cloned().map(Value::String));
                        map.insert("column_names".into(), Value::Array(next));
                        break;
                    }
                }
            }
            map.values_mut().for_each(|c| rename_columns(c, params, columns));
        }
        Value::Array(items) => items.iter_mut().for_each(|c| rename_columns(c, params, columns)),
        _ => {}
    }
}

/// The type set a column of `family` asks for: numbers of any kind (a CSV
/// file read as BIGINT one day and DOUBLE the next), otherwise its own.
pub fn type_set_of(family: TypeFamily) -> Vec<String> {
    use TypeFamily as F;
    match family {
        F::Integer | F::Decimal | F::Float => vec!["number".into()],
        F::Timestamp | F::TimestampTz => vec!["datetime".into()],
        F::Map | F::Binary | F::Other => Vec::new(),
        f => vec![family_name(f)],
    }
}

/// `Order Date` → `order_date`: a parameter name for a column.
fn param_name(column: &str, taken: &BTreeSet<String>, reserved: &BTreeSet<String>) -> String {
    let mut base: String = column
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if base.is_empty() || base.starts_with(|c: char| c.is_ascii_digit()) {
        base = format!("col_{base}").trim_end_matches('_').to_string();
    }
    base.truncate(32);
    let free = |n: &str| is_valid_param_name(n) && !taken.contains(n) && !reserved.contains(n) && !n.contains("__pv_");
    if free(&base) {
        return base;
    }
    (1..)
        .map(|i| if i == 1 { format!("{base}_col") } else { format!("{base}_col{i}") })
        .find(|n| free(n))
        .unwrap_or(base)
}

/// `__pv_` is reserved in bodies: the hidden columns some steps add
/// (`__pv_rn`) are renamed `_pv_…`.
fn unreserve(sql: &str) -> String {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new("(?i)__pv_").expect("pattern")).replace_all(sql, "_pv_").into_owned()
}

/// A name for the operation: `<table>_steps`.
fn suggested_name(table: &str) -> String {
    let mut n: String = table
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if !n.starts_with(|c: char| c.is_ascii_lowercase()) {
        n = format!("t_{n}");
    }
    n.truncate(33);
    format!("{}_steps", n.trim_end_matches('_'))
}

/// Turn the captured steps into a custom operation, the input columns in
/// `params` becoming column parameters. Runs on its own small database
/// (no workspace, no user data beyond the captured rows).
pub fn draft(capture: &StepsCapture, params: &[String]) -> Result<StepsDraft> {
    let conn = validation_connection()?;
    let reserved = reserved_words(&conn)?;
    let input_names: Vec<String> = capture.columns.iter().map(|c| c.name.clone()).collect();
    let table_param = if capture.table.eq_ignore_ascii_case("t") { "input" } else { "t" }.to_string();

    // 1. The chain of steps.
    let mut known: HashMap<String, String> = HashMap::from([(capture.base_query.clone(), table_param.clone())]);
    let mut names: BTreeSet<String> = BTreeSet::from([table_param.clone()]);
    let mut pieces: Vec<(String, Value)> = Vec::new();
    let mut notes = Vec::new();
    let mut stopped = false;
    let mut ordered = false;
    for step in &capture.steps {
        let mut out = StepNote { label: step.label.clone(), included: false, note: None };
        let (source, sql, op) = match &step.kind {
            CapturedKind::CellEdits => {
                out.note = note("custom.steps.cell_edits", &[]);
                notes.push(out);
                continue;
            }
            CapturedKind::Formulas | CapturedKind::SourceReading => {
                out.note = note("custom.steps.under", &[]);
                notes.push(out);
                continue;
            }
            _ if stopped => {
                out.note = note("custom.steps.after_stop", &[]);
                notes.push(out);
                continue;
            }
            CapturedKind::Unrecorded => {
                out.note = note("custom.steps.unrecorded", &[]);
                stopped = true;
                notes.push(out);
                continue;
            }
            CapturedKind::Query { source, sql, op } => (source, sql, op),
        };
        let piece = match source {
            Some(src) => match known.get(src) {
                Some(from) => sql.replace(src.as_str(), &format!("SELECT * FROM {from}")),
                None => {
                    out.note = note("custom.steps.not_separable", &[]);
                    stopped = true;
                    notes.push(out);
                    continue;
                }
            },
            None => sql.clone(),
        };
        let Some(mut tree) = serialize(&conn, &piece) else {
            out.note = note("custom.steps.not_separable", &[]);
            stopped = true;
            notes.push(out);
            continue;
        };
        if source.is_none() {
            // SQL mode: the tab's name is the table under the steps.
            rename_tables(&mut tree, &HashMap::from([(capture.table.to_lowercase(), table_param.clone())]));
        }
        let mut own = BTreeSet::new();
        ctes(&tree, &mut own);
        let mut read = BTreeSet::new();
        base_tables(&tree, &mut read);
        if let Some(other) = read.iter().find(|t| !own.contains(*t) && !names.contains(*t)) {
            let shown = other.trim_end_matches('.').to_string();
            out.note = note("custom.steps.other_table", &[("table", &shown)]);
            stopped = true;
            notes.push(out);
            continue;
        }
        let cte = format!("step_{}", pieces.len() + 1);
        known.insert(sql.clone(), cte.clone());
        names.insert(cte.clone());
        ordered |= matches!(op, Some(Operation::Sort { .. } | Operation::SortBy { .. }))
            || out.label.key == "step.sort"
            || out.label.key == "step.sort_by";
        if matches!(op, Some(Operation::SplitColumn { parts: None, .. }) | Some(Operation::Bin { .. })) {
            out.note = note("custom.steps.fixed_values", &[]);
        }
        out.included = true;
        pieces.push((cte, tree));
        notes.push(out);
    }

    // 2. The columns the steps read and write.
    let mut read = BTreeSet::new();
    let mut written = BTreeSet::new();
    for (_, tree) in &pieces {
        column_use(tree, &input_names, &mut read, &mut written);
    }
    let mut taken: BTreeSet<String> = names.clone();
    taken.extend(input_names.iter().map(|c| c.to_lowercase()));
    taken.extend(written.iter().cloned());
    let mut renames: HashMap<String, String> = HashMap::new();
    let mut choices = Vec::new();
    for c in &capture.columns {
        let lower = c.name.to_lowercase();
        if !read.contains(&lower) {
            continue;
        }
        let fixed = if written.contains(&lower) {
            note("custom.steps.column_written", &[])
        } else if c.name.to_lowercase().starts_with("__pv_") {
            note("custom.steps.column_reserved", &[])
        } else {
            None
        };
        let param = if fixed.is_none() && params.iter().any(|p| p == &c.name) {
            // The parameter may keep the column's own name.
            let mut others = taken.clone();
            others.remove(&lower);
            let p = param_name(&c.name, &others, &reserved);
            taken.insert(p.clone());
            renames.insert(c.name.clone(), p.clone());
            Some(p)
        } else {
            None
        };
        choices.push(ColumnChoice { name: c.name.clone(), dtype: c.dtype.clone(), param, fixed });
    }
    if pieces.is_empty() {
        return Ok(StepsDraft { definition: None, steps: notes, columns: choices, problem: None });
    }

    // 3. The body: the pieces as CTEs, the column parameters renamed.
    let mut sqls = Vec::new();
    for (cte, tree) in &mut pieces {
        if !renames.is_empty() {
            rename_columns(tree, &renames, &input_names);
        }
        let text = deserialize(&conn, tree).ok_or_else(|| crate::CoreError::invalid("Could not print a step's SQL"))?;
        sqls.push((cte.clone(), unreserve(&text)));
    }
    let source = if sqls.len() == 1 {
        sqls[0].1.clone()
    } else {
        let ctes: Vec<String> = sqls.iter().map(|(n, q)| format!("{n} AS (\n  {q}\n)")).collect();
        format!("WITH {}\nSELECT * FROM {}", ctes.join(",\n"), sqls.last().map(|(n, _)| n.as_str()).unwrap_or_default())
    };

    // 4. The signature.
    let mut required = Vec::new();
    let mut column_params = Vec::new();
    for choice in &choices {
        let family = TypeFamily::of(&choice.dtype);
        match &choice.param {
            Some(p) => column_params.push(Param {
                name: p.clone(),
                kind: ParamKind::Column,
                types: type_set_of(family),
                description: choice.name.clone(),
                ..Default::default()
            }),
            None => required.push(RequiredColumn { name: choice.name.clone(), types: type_set_of(family) }),
        }
    }
    let mut def_params = vec![Param { name: table_param.clone(), kind: ParamKind::Table, columns: required, ..Default::default() }];
    def_params.extend(column_params);
    let mut def = Definition {
        kind: DefKind::Operation,
        name: suggested_name(&capture.table),
        status: Status::Draft,
        params: def_params,
        result: ResultSpec { kind: ResultKind::Table, ..Default::default() },
        placement: Some(Placement::Step),
        body: Body { source, ..Default::default() },
        ..Default::default()
    };

    // 5. An example: the first rows, and what the steps make of them.
    let used: Vec<usize> = capture
        .columns
        .iter()
        .enumerate()
        .filter(|(_, c)| choices.iter().any(|x| x.name == c.name))
        .map(|(i, _)| i)
        .collect();
    let keep: Vec<usize> = if used.is_empty() { (0..capture.columns.len()).collect() } else { used };
    let keep: Vec<usize> = keep.into_iter().take(MAX_EXAMPLE_COLUMNS).collect();
    let example = |n: usize| Example {
        name: "first rows".into(),
        input: ExampleTable {
            columns: keep
                .iter()
                .map(|&i| ExampleColumn { name: capture.columns[i].name.clone(), dtype: capture.columns[i].dtype.clone() })
                .collect(),
            rows: capture.rows.iter().take(n).map(|r| keep.iter().map(|&i| r.get(i).cloned().flatten()).collect()).collect(),
        },
        args: renames.iter().map(|(col, p)| (p.clone(), ExampleArg::Cell(Some(col.clone())))).collect(),
        ordered,
        ..Default::default()
    };
    let stability = function_stability(&conn)?;
    let mut problem = None;
    let mut ex = example(EXAMPLE_ROWS);
    match compile(&conn, &def, &stability) {
        Ok(out) => {
            for n in [EXAMPLE_ROWS, SAMPLE_ROWS] {
                ex = example(n);
                let outcome = run_example(&conn, &def, &out.template, &ex);
                match (outcome.actual, outcome.error) {
                    (Some(actual), _) => {
                        let empty = actual.rows.is_empty();
                        ex.expected = ExampleTable {
                            columns: actual.columns.into_iter().map(|c| ExampleColumn { name: c.name, dtype: String::new() }).collect(),
                            rows: actual.rows,
                        };
                        if !empty || capture.rows.len() <= n {
                            break;
                        }
                    }
                    (None, e) => {
                        problem = e;
                        break;
                    }
                }
            }
        }
        Err(issue) => problem = Some(format!("{}: {:?}", issue.key, issue.args)),
    }
    def.examples = vec![ex];
    Ok(StepsDraft { definition: Some(def), steps: notes, columns: choices, problem })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_names() {
        let none = BTreeSet::new();
        assert_eq!(param_name("Order Date", &none, &none), "order_date");
        assert_eq!(param_name("2024", &none, &none), "col_2024");
        let taken = BTreeSet::from(["region".to_string()]);
        assert_eq!(param_name("Region", &taken, &none), "region_col");
        let reserved = BTreeSet::from(["order".to_string()]);
        assert_eq!(param_name("order", &none, &reserved), "order_col");
        assert_eq!(suggested_name("Sales 2024"), "sales_2024_steps");
        assert_eq!(suggested_name("2024"), "t_2024_steps");
        assert_eq!(unreserve("SELECT \"__pv_rn\" FROM x"), "SELECT \"_pv_rn\" FROM x");
    }
}
