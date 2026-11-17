//! Strict validation before saving (plans/CUSTOM_TOOLS_PLAN.md §6).
//!
//! Runs on its own small in-memory DuckDB, never on a workspace: it holds
//! no lock and cannot see user data. Each example is interrupted after a
//! few seconds.

use std::collections::BTreeMap;
use std::sync::mpsc;
use std::time::Duration;

use duckdb::Connection;
use serde::{Deserialize, Serialize};

use super::compile::{ARGS_TABLE, check_text, compile, function_stability, reserved_words};
use super::examples::{ExampleOutcome, clean_error, run_example};
use super::fill::{check_table_result, fill_slots, value_literal, with_tables};
use super::signature::{check_signature, families_of, family_name, is_valid_name, probe_families, probe_type};
use super::{
    Body, BodyLanguage, Compiled, DefKind, Definition, MAX_EXAMPLE_COLUMNS, MAX_EXAMPLE_ROWS, MAX_EXAMPLES, ParamKind, ResultKind, Shape,
};
use crate::engine::describe;
use crate::page::ColumnMeta;
use crate::sql::quote_ident;
use crate::types::TypeFamily;

/// A message as a translation key plus arguments (the step label shape).
pub type Issue = crate::ops::StepLabel;

/// How long one example may run.
const EXAMPLE_TIMEOUT: Duration = Duration::from_secs(5);
/// Most type probes; above it each parameter is probed on its own.
const MAX_PROBES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Warning,
    Fail,
    Skipped,
}

/// One line of the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    /// Which check (`name`, `signature`, `body`, `safety`, `types`, `shape`,
    /// `examples`, `stable`).
    pub id: String,
    pub status: CheckStatus,
    pub message: Issue,
    /// The example the line is about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<usize>,
}

/// What validation found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Report {
    pub checks: Vec<Check>,
    #[serde(default)]
    pub examples: Vec<ExampleOutcome>,
    /// The compiled form, when the body compiled and its types checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiled: Option<Compiled>,
}

impl Report {
    /// No check failed.
    pub fn passed(&self) -> bool {
        !self.checks.iter().any(|c| c.status == CheckStatus::Fail) && self.compiled.is_some()
    }

    fn add(&mut self, id: &str, status: CheckStatus, message: Issue) {
        self.checks.push(Check { id: id.into(), status, message, example: None });
    }

    fn pass(&mut self, id: &str, key: &str) {
        self.add(id, CheckStatus::Pass, Issue::new(key, &[]));
    }

    fn fail(&mut self, id: &str, message: Issue) {
        self.add(id, CheckStatus::Fail, message);
    }

    fn skip(&mut self, ids: &[&str]) {
        for id in ids {
            self.add(id, CheckStatus::Skipped, Issue::new("custom.check.skipped", &[]));
        }
    }

    /// The first failure, as one line of English for errors.
    pub fn first_failure(&self) -> Option<&Check> {
        self.checks.iter().find(|c| c.status == CheckStatus::Fail)
    }
}

/// Translates an expression-language body to a SQL expression.
pub type ExpressionSql = dyn Fn(&Definition) -> Result<String, Issue>;

/// What validation needs from outside the core.
pub struct ValidateEnv<'a> {
    /// Why a name is taken (a built-in expression function, another custom
    /// tool…), or `None`.
    pub name_taken: &'a dyn Fn(&str) -> Option<String>,
    /// The app's version, stored with the compiled form.
    pub app_version: &'a str,
    /// Translate an expression-language body to a SQL expression (the
    /// expression language lives in `parvu-dsl`, which this crate doesn't
    /// use). `None`: expression bodies can't be validated here.
    pub expression_sql: Option<&'a ExpressionSql>,
}

/// A connection for validation: in memory, small, two threads.
pub fn validation_connection() -> crate::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    crate::interrupts::register(&conn);
    conn.execute_batch("SET memory_limit = '256MB'; SET threads = 2; SET enable_external_access = false;")?;
    Ok(conn)
}

/// DuckDB's version (`v2.0.0`).
pub fn duckdb_version(conn: &Connection) -> String {
    conn.query_row("SELECT version()", [], |r| r.get::<_, String>(0)).unwrap_or_default()
}

/// Run every check of §6 on `def`. Nothing is written.
pub fn validate(def: &Definition, env: &ValidateEnv) -> crate::Result<Report> {
    let conn = validation_connection()?;
    let stability = function_stability(&conn)?;
    let reserved = reserved_words(&conn)?;
    let mut report = Report::default();

    // 1. Name
    let lower = def.name.to_lowercase();
    if !is_valid_name(&def.name) {
        report.fail("name", Issue::new("custom.check.name_format", &[("name", &def.name)]));
    } else if reserved.contains(&lower) {
        report.fail("name", Issue::new("custom.check.name_keyword", &[("name", &def.name)]));
    } else if stability.contains_key(&lower) {
        report.fail("name", Issue::new("custom.check.name_duckdb", &[("name", &def.name)]));
    } else if let Some(why) = (env.name_taken)(&lower) {
        report.fail("name", Issue::new("custom.check.name_taken", &[("name", &def.name), ("why", &why)]));
    } else {
        report.pass("name", "custom.check.name_ok");
    }

    // 2. Signature
    let issues = check_signature(def, &|w| reserved.contains(w));
    if issues.is_empty() {
        report.pass("signature", "custom.check.signature_ok");
    } else {
        for i in issues {
            report.fail("signature", i);
        }
        report.skip(&["body", "types", "shape", "examples"]);
        return Ok(report);
    }

    // An expression body becomes SQL first; the SQL then takes every check.
    // The hash stays the one of the source as written.
    let source = def;
    let translated;
    let def = match def.body.language {
        BodyLanguage::Sql => def,
        BodyLanguage::Expression => {
            let sql = if def.kind != DefKind::Function {
                Err(Issue::new("custom.check.expression_function_only", &[]))
            } else if let Some(issue) = check_text(&def.body.source) {
                Err(issue)
            } else {
                match env.expression_sql {
                    Some(f) => f(def),
                    None => Err(Issue::new("custom.check.expression_unavailable", &[])),
                }
            };
            match sql {
                Ok(sql) => {
                    let mut d = def.clone();
                    d.body = Body { language: BodyLanguage::Sql, source: sql };
                    translated = d;
                    &translated
                }
                Err(issue) => {
                    report.fail("body", issue);
                    report.skip(&["types", "shape", "examples"]);
                    return Ok(report);
                }
            }
        }
    };

    // 3–6. Body, safety, determinism (compile)
    let out = match compile(&conn, def, &stability) {
        Ok(o) => o,
        Err(issue) => {
            let id = if issue.key == "custom.check.safety" || issue.key == "custom.check.volatile" { "safety" } else { "body" };
            report.fail(id, issue);
            report.skip(&["types", "shape", "examples"]);
            return Ok(report);
        }
    };
    report.pass("body", "custom.check.body_ok");
    for w in &out.warnings {
        report.add("body", CheckStatus::Warning, w.clone());
    }
    if out.uses_current_time {
        report.add("body", CheckStatus::Warning, Issue::new("custom.check.uses_today", &[]));
    }

    // 7–8. Types and shape, on empty typed probes
    let shape = match def.kind {
        DefKind::Operation => Shape::Table,
        DefKind::Function if def.result.kind == ResultKind::Value => Shape::Summary,
        DefKind::Function => Shape::Row,
    };
    let mut result_types = BTreeMap::new();
    let types_ok = match probe_all(&conn, def, &out.template, shape, &mut result_types) {
        Ok(()) => {
            report.pass("types", "custom.check.types_ok");
            true
        }
        Err(issue) => {
            report.fail("types", issue);
            false
        }
    };
    if def.kind == DefKind::Function {
        match check_shape(&conn, def, &out.template, shape) {
            Ok(()) => report.pass("shape", "custom.check.shape_ok"),
            Err(issue) => report.fail("shape", issue),
        }
    }

    // 11. Examples
    if def.examples.is_empty() {
        report.fail("examples", Issue::new("custom.check.no_examples", &[]));
    } else if def.examples.len() > MAX_EXAMPLES {
        report.fail("examples", Issue::new("custom.check.too_many_examples", &[("max", &MAX_EXAMPLES.to_string())]));
    } else {
        let mut has_null = false;
        for (i, ex) in def.examples.iter().enumerate() {
            let n = (i + 1).to_string();
            let all_tables = std::iter::once(&ex.input).chain(ex.tables.values());
            if all_tables.clone().any(|t| t.rows.len() > MAX_EXAMPLE_ROWS || t.columns.len() > MAX_EXAMPLE_COLUMNS) {
                report.checks.push(Check {
                    id: "examples".into(),
                    status: CheckStatus::Fail,
                    message: Issue::new("custom.check.example_too_big", &[("example", &n)]),
                    example: Some(i),
                });
                        report.examples.push(ExampleOutcome { name: ex.name.clone(), ..Default::default() });
                continue;
            }
            has_null |= all_tables.flat_map(|t| t.rows.iter()).any(|r| r.iter().any(Option::is_none));
            let outcome = with_timeout(&conn, EXAMPLE_TIMEOUT, || run_example(&conn, def, &out.template, ex));
            let (status, message) = if outcome.passed {
                (CheckStatus::Pass, Issue::new("custom.check.example_passed", &[("example", &n)]))
            } else if let Some(err) = &outcome.error {
                (CheckStatus::Fail, Issue::new("custom.check.example_error", &[("example", &n), ("error", err)]))
            } else {
                (
                    CheckStatus::Fail,
                    Issue::new("custom.check.example_differs", &[("example", &n), ("count", &outcome.mismatches.len().to_string())]),
                )
            };
            report.checks.push(Check { id: "examples".into(), status, message, example: Some(i) });
            report.examples.push(outcome);
        }
        if !has_null {
            report.add("examples", CheckStatus::Warning, Issue::new("custom.check.no_empty_example", &[]));
        }
    }

    // 12. Stable compile
    match compile(&conn, def, &stability) {
        Ok(again) if again.template == out.template => report.pass("stable", "custom.check.stable_ok"),
        _ => report.fail("stable", Issue::new("custom.check.unstable", &[])),
    }

    if types_ok {
        report.compiled = Some(Compiled {
            template: out.template,
            hash: source.source_hash(),
            shape,
            reads_other_rows: out.reads_other_rows,
            result_types,
            app: env.app_version.to_string(),
            duckdb: duckdb_version(&conn),
        });
    }
    Ok(report)
}

/// Run `f`, interrupting the connection's query after `limit`.
fn with_timeout<T>(conn: &Connection, limit: Duration, f: impl FnOnce() -> T) -> T {
    let (tx, rx) = mpsc::channel::<()>();
    let handle = conn.interrupt_handle();
    let watchdog = std::thread::spawn(move || {
        if rx.recv_timeout(limit) == Err(mpsc::RecvTimeoutError::Timeout) {
            handle.interrupt();
        }
    });
    let out = f();
    drop(tx);
    let _ = watchdog.join();
    out
}

/// One combination of probe types: the DuckDB type of each column
/// parameter (by index), plus a description such as `x=number:BIGINT`.
struct Probe {
    types: BTreeMap<usize, &'static str>,
    families: BTreeMap<usize, TypeFamily>,
    label: String,
}

/// Every combination of the column parameters' families, or one family at
/// a time when there are too many.
fn probes(def: &Definition) -> Vec<Probe> {
    let cols: Vec<(usize, Vec<TypeFamily>)> = def
        .params
        .iter()
        .enumerate()
        .filter(|(_, p)| p.kind == ParamKind::Column)
        .map(|(i, p)| (i, probe_families(&p.types)))
        .collect();
    let total: usize = cols.iter().map(|(_, f)| f.len().max(1)).product();
    let mut combos: Vec<BTreeMap<usize, TypeFamily>> = Vec::new();
    if total <= MAX_PROBES {
        combos.push(BTreeMap::new());
        for (i, fams) in &cols {
            combos = combos
                .into_iter()
                .flat_map(|c| {
                    fams.iter().map(move |f| {
                        let mut n = c.clone();
                        n.insert(*i, *f);
                        n
                    })
                })
                .collect();
        }
    } else {
        let base: BTreeMap<usize, TypeFamily> = cols.iter().map(|(i, f)| (*i, f[0])).collect();
        for (i, fams) in &cols {
            for f in fams {
                let mut c = base.clone();
                c.insert(*i, *f);
                if !combos.contains(&c) {
                    combos.push(c);
                }
            }
        }
    }
    combos
        .into_iter()
        .map(|families| {
            let label = families
                .iter()
                .map(|(i, f)| format!("{}={}", def.params[*i].name, family_name(*f)))
                .collect::<Vec<_>>()
                .join(", ");
            Probe { types: families.iter().map(|(i, f)| (*i, probe_type(*f))).collect(), families, label }
        })
        .collect()
}

/// `SELECT CAST(NULL AS T) AS "c", … WHERE false`.
fn empty_relation(columns: &[(String, String)]) -> String {
    if columns.is_empty() {
        return "SELECT 1 AS __pv_empty WHERE false".into();
    }
    let items: Vec<String> = columns.iter().map(|(n, t)| format!("CAST(NULL AS {t}) AS {}", quote_ident(n))).collect();
    format!("SELECT {} WHERE false", items.join(", "))
}

/// The SQL a probe binds: the template over empty typed inputs.
fn probe_sql(def: &Definition, template: &str, probe: &Probe, extra_row: bool, use_defaults: bool) -> crate::Result<String> {
    let mut slots = vec![String::new(); def.params.len()];
    for (i, p) in def.params.iter().enumerate() {
        match p.kind {
            ParamKind::Column => slots[i] = quote_ident(&p.name),
            ParamKind::Value => {
                let raw = if use_defaults { p.default.as_deref() } else { None };
                slots[i] = format!("({})", value_literal(p, raw)?);
            }
            ParamKind::Table => {}
        }
    }
    let filled = fill_slots(template, &|i| slots.get(i).cloned().unwrap_or_default());
    match def.kind {
        DefKind::Function => {
            let mut cols: Vec<(String, String)> = def
                .params
                .iter()
                .enumerate()
                .filter_map(|(i, p)| probe.types.get(&i).map(|t| (p.name.clone(), t.to_string())))
                .collect();
            if extra_row {
                cols.push(("__pv_rn".into(), "BIGINT".into()));
            }
            let rn = if extra_row { ", \"__pv_rn\"" } else { "" };
            Ok(format!("SELECT {filled} AS result{rn} FROM ({}) AS {}", empty_relation(&cols), quote_ident(ARGS_TABLE)))
        }
        DefKind::Operation => {
            let mut tables = Vec::new();
            for t in def.tables() {
                let mut cols: Vec<(String, String)> = t
                    .columns
                    .iter()
                    .map(|c| (c.name.clone(), probe_type(probe_families(&c.types)[0]).to_string()))
                    .collect();
                for (i, p) in def.params.iter().enumerate() {
                    let belongs = def.table_of(p).is_some_and(|x| x.name == t.name);
                    if p.kind == ParamKind::Column && belongs && let Some(ty) = probe.types.get(&i) {
                        cols.push((p.name.clone(), ty.to_string()));
                    }
                }
                tables.push(empty_relation(&cols));
            }
            Ok(with_tables(&tables, &filled))
        }
    }
}

/// Check 7: every probe binds, and its result type is the declared one.
fn probe_all(
    conn: &Connection,
    def: &Definition,
    template: &str,
    shape: Shape,
    result_types: &mut BTreeMap<String, String>,
) -> Result<(), Issue> {
    let declared = families_of(&def.result.types).unwrap_or_default();
    let has_defaults = def.params.iter().any(|p| p.default.is_some());
    for probe in probes(def) {
        for use_defaults in [false, true] {
            if use_defaults && !has_defaults {
                continue;
            }
            let sql = probe_sql(def, template, &probe, false, use_defaults)
                .map_err(|e| Issue::new("custom.check.types_bind", &[("probe", &probe.label), ("error", &e.to_string())]))?;
            let cols = describe(conn, &sql).map_err(|e| {
                Issue::new("custom.check.types_bind", &[("probe", &probe.label), ("error", &clean_error(&e.to_string()))])
            })?;
            if use_defaults {
                continue;
            }
            match shape {
                Shape::Table => {
                    check_table_result(def, &cols)
                        .map_err(|e| Issue::new("custom.check.table_result", &[("probe", &probe.label), ("error", &e.to_string())]))?;
                    let names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
                    result_types.insert(probe.label.clone(), names.join(", "));
                }
                _ => {
                    let meta: &ColumnMeta = cols.first().ok_or_else(|| Issue::new("custom.check.one_expression", &[]))?;
                    let ok = match &def.result.same_as {
                        Some(p) => {
                            let i = def.params.iter().position(|x| x.name.eq_ignore_ascii_case(p)).unwrap_or(usize::MAX);
                            probe.families.get(&i).is_none_or(|f| *f == meta.family)
                        }
                        None => declared.contains(&meta.family),
                    };
                    if !ok {
                        let wanted = match &def.result.same_as {
                            Some(p) => format!("same as {p}"),
                            None if def.result.types.is_empty() => "any".into(),
                            None => def.result.types.join(" or "),
                        };
                        return Err(Issue::new(
                            "custom.check.result_type",
                            &[("probe", &probe.label), ("type", &meta.dtype), ("expected", &wanted)],
                        ));
                    }
                    result_types.insert(if probe.label.is_empty() { "-".into() } else { probe.label.clone() }, meta.dtype.clone());
                }
            }
        }
    }
    Ok(())
}

/// Check 8: a row function must bind next to a plain column (so it doesn't
/// aggregate); a summary function must not, and must bind on its own.
fn check_shape(conn: &Connection, def: &Definition, template: &str, shape: Shape) -> Result<(), Issue> {
    let Some(probe) = probes(def).into_iter().next() else { return Ok(()) };
    let with_row = probe_sql(def, template, &probe, true, false).map_err(|e| Issue::new("custom.check.parse", &[("error", &e.to_string())]))?;
    let grouped_error = |e: &crate::CoreError| {
        let m = e.to_string();
        m.contains("Binder Error") && m.contains("GROUP BY")
    };
    match (shape, describe(conn, &with_row)) {
        (Shape::Row, Ok(_)) => Ok(()),
        (Shape::Row, Err(e)) if grouped_error(&e) => Err(Issue::new("custom.check.row_aggregates", &[])),
        (Shape::Row, Err(e)) => Err(Issue::new("custom.check.parse", &[("error", &clean_error(&e.to_string()))])),
        (Shape::Summary, Ok(_)) => Err(Issue::new("custom.check.summary_not_aggregate", &[])),
        (Shape::Summary, Err(e)) if grouped_error(&e) => Ok(()),
        (Shape::Summary, Err(e)) => Err(Issue::new("custom.check.parse", &[("error", &clean_error(&e.to_string()))])),
        (Shape::Table, _) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom::{Body, Example, ExampleColumn, ExampleTable, Param, ResultSpec};

    fn env() -> ValidateEnv<'static> {
        ValidateEnv {
            name_taken: &|n| (n == "sum_all").then(|| "a built-in function".to_string()),
            app_version: "test",
            expression_sql: None,
        }
    }

    fn table(cols: &[(&str, &str)], rows: &[&[Option<&str>]]) -> ExampleTable {
        ExampleTable {
            columns: cols.iter().map(|(n, t)| ExampleColumn { name: n.to_string(), dtype: t.to_string() }).collect(),
            rows: rows.iter().map(|r| r.iter().map(|c| c.map(str::to_string)).collect()).collect(),
        }
    }

    fn double() -> Definition {
        Definition {
            name: "double_it".into(),
            params: vec![Param { name: "x".into(), types: vec!["number".into()], ..Default::default() }],
            result: ResultSpec { types: vec!["number".into()], ..Default::default() },
            body: Body { source: "x * 2".into(), ..Default::default() },
            examples: vec![Example {
                input: table(&[("x", "BIGINT")], &[&[Some("2")], &[None]]),
                expected: table(&[("result", "")], &[&[Some("4")], &[None]]),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn a_good_function_passes() {
        let r = validate(&double(), &env()).unwrap();
        assert!(r.passed(), "{:#?}", r.checks);
        let c = r.compiled.unwrap();
        assert_eq!(c.template, "(__pv_arg_0 * 2)");
        assert_eq!(c.shape, Shape::Row);
        assert_eq!(c.result_types.len(), 3, "integer, decimal, float probes");
    }

    #[test]
    fn failures_are_reported() {
        let fail = |d: Definition| {
            let r = validate(&d, &env()).unwrap();
            assert!(!r.passed());
            r.first_failure().unwrap().message.key.clone()
        };
        let mut d = double();
        d.name = "sum_all".into();
        assert_eq!(fail(d), "custom.check.name_taken");
        let mut d = double();
        d.name = "length".into();
        assert_eq!(fail(d), "custom.check.name_duckdb");
        let mut d = double();
        d.result.types = vec!["text".into()];
        assert_eq!(fail(d), "custom.check.result_type");
        let mut d = double();
        d.body.source = "sum(x)".into();
        assert_eq!(fail(d), "custom.check.row_aggregates");
        let mut d = double();
        d.examples[0].expected.rows[0][0] = Some("5".into());
        assert_eq!(fail(d), "custom.check.example_differs");
        let mut d = double();
        d.examples.clear();
        assert_eq!(fail(d), "custom.check.no_examples");
    }

    #[test]
    fn expression_bodies_are_translated_then_checked() {
        let mut d = double();
        d.body = Body { language: BodyLanguage::Expression, source: "[x] * 2".into() };
        // Without a translator: refused.
        let r = validate(&d, &env()).unwrap();
        assert_eq!(r.first_failure().unwrap().message.key, "custom.check.expression_unavailable");
        // With one: the SQL it gives takes every check; the hash is the source's.
        let tr = |def: &Definition| {
            assert_eq!(def.body.source, "[x] * 2");
            Ok("x * 2".to_string())
        };
        let with = ValidateEnv { expression_sql: Some(&tr), ..env() };
        let r = validate(&d, &with).unwrap();
        assert!(r.passed(), "{:#?}", r.checks);
        let c = r.compiled.unwrap();
        assert_eq!(c.template, "(__pv_arg_0 * 2)");
        assert_eq!(c.hash, d.source_hash());
        // The translation's error is the body's failure; `__pv_` in the source is refused first.
        let bad = |_: &Definition| Err(Issue::new("custom.check.parse", &[("error", "nope")]));
        let r = validate(&d, &ValidateEnv { expression_sql: Some(&bad), ..env() }).unwrap();
        assert_eq!(r.first_failure().unwrap().id, "body");
        d.body.source = "[__pv_x]".into();
        let r = validate(&d, &ValidateEnv { expression_sql: Some(&tr), ..env() }).unwrap();
        assert_eq!(r.first_failure().unwrap().message.key, "custom.check.body_reserved");
    }
}
