//! Commands for custom functions and operations (plans/CUSTOM_TOOLS_PLAN.md
//! §7, §8): list, validate, save, delete, import, export, and running a
//! summary function or a preview as a job.
//!
//! Validation runs on the blocking pool on its own small DuckDB, never
//! under a workspace lock. A change updates every window's workspace and
//! emits `custom-changed`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use parvu_core::custom::fill::{fill_slots, value_literal};
use parvu_core::custom::validate::Issue;
use parvu_core::custom::{Bundle, CustomArg, Definition, Library, Report, Status, ValidateEnv, families_of, validate};
use parvu_core::formula::Formula;
use parvu_core::ops::Operation;
use parvu_core::types::TypeFamily;
use parvu_core::{TabId, Workspace};
use parvu_dsl::{Catalog, FunctionDef, FunctionRegistry, LogicalType, ParamDef};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State, WebviewWindow};

use crate::custom_store::{Clash, CustomStore, Entry, ImportResult, checked};
use crate::error::{ApiError, ApiResult};
use crate::jobs::run_job;
use crate::state::{AppState, lock};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Names the expression language gives a meaning of its own.
const SPECIAL_FORMS: &[&str] = &["drop_duplicates", "drop_null", "replace", "xlookup", "vlookup", "text", "prev", "next"];

/// The running DuckDB's version, for the compiled-form cache.
pub fn duckdb_version() -> &'static str {
    static V: OnceLock<String> = OnceLock::new();
    V.get_or_init(|| {
        parvu_core::custom::validate::validation_connection()
            .map(|c| parvu_core::custom::validate::duckdb_version(&c))
            .unwrap_or_default()
    })
}

/// Why `name` cannot name a custom tool (as a translation key), with the
/// definition being edited (`editing`) not counting as taken.
fn name_taken(store: &CustomStore, editing: Option<&str>, name: &str) -> Option<String> {
    if SPECIAL_FORMS.contains(&name) || parvu_dsl::FunctionRegistry::default().lookup(name).is_some() {
        return Some("custom.taken.expression".into());
    }
    if parvu_dsl::excel::is_excel(name) {
        return Some("custom.taken.excel".into());
    }
    let mine = editing.is_some_and(|e| e.eq_ignore_ascii_case(name));
    if !mine && store.get(name).is_some() {
        return Some("custom.taken.custom".into());
    }
    None
}

// ---------------------------------------------------------------- expression language

/// The table an expression body's `[x]` refers to (its parameters).
const ARGS_TABLE: &str = "__pv_args";

/// The expression language's type for a set of type families: one known
/// type, or `Unknown`.
fn logical_of(families: &[TypeFamily]) -> LogicalType {
    let mut types = families.iter().map(|f| logical_family(*f));
    match types.next() {
        Some(first) if types.all(|t| t == first) => first,
        _ => LogicalType::Unknown,
    }
}

fn logical_family(f: TypeFamily) -> LogicalType {
    use TypeFamily as F;
    match f {
        F::Integer | F::Decimal | F::Float => LogicalType::Numeric,
        F::Text => LogicalType::Text,
        F::Boolean => LogicalType::Boolean,
        F::Date => LogicalType::Date,
        F::Timestamp | F::TimestampTz => LogicalType::Timestamp,
        F::List => LogicalType::List,
        F::Struct => LogicalType::Struct,
        _ => LogicalType::Unknown,
    }
}

/// The argument types a column parameter accepts, when the expression
/// language can tell them apart (not for `any`, JSON, times…).
fn accepted(types: &[String]) -> Option<Vec<LogicalType>> {
    let families = families_of(types).ok()?;
    let mut out: Vec<LogicalType> = Vec::new();
    for f in families {
        let t = logical_family(f);
        if t == LogicalType::Unknown {
            return None;
        }
        if !out.contains(&t) {
            out.push(t);
        }
    }
    Some(out)
}

/// A usable custom function as the expression language calls it: typed
/// column parameters, value parameters cast to their declared type (with
/// their default), the template's `__pv_arg_N` slots, and the summary and
/// window flags. `None` for operations and anything not usable.
pub fn function_def(def: &Definition) -> Option<FunctionDef> {
    use parvu_core::custom::{DefKind, ParamKind, Shape};
    let compiled = def.compiled.as_ref().filter(|_| def.is_usable() && def.kind == DefKind::Function)?;
    // A list of columns can't be passed in an expression (yet).
    if def.params.iter().any(|p| p.kind == ParamKind::Columns) {
        return None;
    }
    let mut params = Vec::new();
    for p in &def.params {
        params.push(match p.kind {
            ParamKind::Column => {
                let mut pd = ParamDef::new(&p.name, logical_of(&families_of(&p.types).ok()?));
                pd.accepts = accepted(&p.types);
                pd.description = p.description.clone();
                pd
            }
            ParamKind::Value => {
                let dtype = p.dtype.as_deref().unwrap_or("VARCHAR");
                if !parvu_core::sql::is_safe_type_name(dtype) {
                    return None;
                }
                let lt = parvu_dsl::duckdb_type_to_logical(dtype);
                let mut pd = if p.optional {
                    ParamDef::with_default(&p.name, lt, &value_literal(p, p.default.as_deref()).ok()?)
                } else {
                    ParamDef::new(&p.name, lt)
                };
                pd.description = p.description.clone();
                pd
            }
            ParamKind::Table | ParamKind::Columns => return None,
        });
    }
    // Value arguments are cast to the type the definition was checked with.
    let template = fill_slots(&compiled.template, &|i| match def.params.get(i) {
        Some(p) if p.kind == ParamKind::Value => format!("CAST(__pv_arg_{i} AS {})", p.dtype.as_deref().unwrap_or("VARCHAR")),
        _ => format!("__pv_arg_{i}"),
    });
    let ret = match &def.result.same_as {
        Some(s) => def.param(s).map(|p| logical_of(&families_of(&p.types).unwrap_or_default())).unwrap_or(LogicalType::Unknown),
        None => logical_of(&families_of(&def.result.types).unwrap_or_default()),
    };
    let text = if def.description.is_empty() { def.title.clone() } else { def.description.clone() };
    let mut f = FunctionDef::custom(&def.name, params, ret, &text, &template);
    f.example = def.signature();
    f.aggregate = compiled.shape == Shape::Summary;
    f.window = compiled.reads_other_rows;
    Some(f)
}

/// The usable custom functions of `lib`, as the expression language sees them.
pub fn custom_functions(lib: &Library) -> Vec<FunctionDef> {
    lib.definitions().iter().filter_map(|d| function_def(d)).collect()
}

/// The expression language's functions for a workspace: the built-ins plus
/// the custom functions of its library. Built once per library.
pub fn registry_for(lib: &Arc<Library>) -> Arc<FunctionRegistry> {
    static CACHE: Mutex<Option<(Weak<Library>, Arc<FunctionRegistry>)>> = Mutex::new(None);
    let mut cache = lock(&CACHE);
    if let Some((weak, reg)) = cache.as_ref()
        && weak.upgrade().is_some_and(|l| Arc::ptr_eq(&l, lib))
    {
        return reg.clone();
    }
    let mut reg = FunctionRegistry::default();
    for f in custom_functions(lib) {
        reg.register(f);
    }
    let reg = Arc::new(reg);
    *cache = Some((Arc::downgrade(lib), reg.clone()));
    reg
}

/// [`registry_for`] the workspace's current library.
pub fn registry(ws: &Workspace) -> Arc<FunctionRegistry> {
    registry_for(ws.custom())
}

/// An expression-language body as the SQL expression validation checks:
/// `[x]` is parameter `x`. Custom functions can't be called from a body
/// (one custom tool doesn't depend on another).
pub fn expression_body_sql(def: &Definition) -> Result<String, Issue> {
    use parvu_core::custom::ParamKind;
    use parvu_core::custom::signature::{probe_families, probe_type};
    let fail = |e: &dyn std::fmt::Display| Issue::new("custom.check.expression_error", &[("error", &e.to_string())]);
    let mut catalog = Catalog::new();
    catalog.register_table(
        ARGS_TABLE,
        def.params.iter().filter(|p| p.kind != ParamKind::Table).map(|p| {
            let ty = match p.kind {
                ParamKind::Value => p.dtype.clone().unwrap_or_else(|| "VARCHAR".into()),
                _ => probe_families(&p.types).first().map(|f| probe_type(*f)).unwrap_or("VARCHAR").to_string(),
            };
            (p.name.clone(), ty)
        }),
    );
    let reg = FunctionRegistry::default();
    let ast = parvu_dsl::parse(def.body.source.trim()).map_err(|e| fail(&e))?;
    let expr = parvu_dsl::resolve_in(&ast, &catalog, &reg, ARGS_TABLE).map_err(|e| fail(&e))?;
    if matches!(expr, parvu_dsl::Expr::Assignment { .. }) || expr.is_table_level() {
        return Err(Issue::new("custom.check.expression_one_value", &[]));
    }
    let sql = parvu_dsl::compile(&expr, &reg).map_err(|e| fail(&e))?;
    let sql = parvu_dsl::strip_table_qualifier(&sql, ARGS_TABLE);
    if sql.contains("__pv_") {
        return Err(Issue::new("custom.check.expression_construct", &[]));
    }
    Ok(sql)
}

// ---------------------------------------------------------------- AI and MCP

/// A check message (a translation key with arguments) in English, for AI clients.
pub fn english(issue: &Issue) -> String {
    let i18n = parvu_config::I18n::new();
    let args: Vec<(&str, parvu_config::FmtArg)> = issue.args.iter().map(|(k, v)| (k.as_str(), v.as_str().into())).collect();
    let text = i18n.tr(&issue.key, &args);
    // A name-taken reason is itself a key.
    match issue.args.get("why") {
        Some(why) if why.starts_with("custom.taken.") => text.replace(why.as_str(), &i18n.t(why)),
        _ => text,
    }
}

/// A validation report as an AI client reads it: each check in English,
/// the examples' outcomes and, when it compiled, the SQL template.
pub fn report_json(report: &Report) -> serde_json::Value {
    use serde_json::json;
    let checks: Vec<_> = report
        .checks
        .iter()
        .map(|c| {
            let mut v = json!({ "check": c.id, "status": c.status, "message": english(&c.message) });
            if let Some(i) = c.example {
                v["example"] = json!(i + 1);
            }
            v
        })
        .collect();
    let examples: Vec<_> = report
        .examples
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let mut v = json!({ "example": i + 1, "passed": o.passed });
            if let Some(e) = &o.error {
                v["error"] = json!(e);
            }
            if !o.passed && o.error.is_none() {
                v["actual"] = json!(o.actual);
                v["mismatches"] = json!(o.mismatches);
            }
            v
        })
        .collect();
    let mut out = json!({ "passed": report.passed(), "checks": checks, "examples": examples });
    if let Some(c) = &report.compiled {
        out["compiled_template"] = json!(c.template);
    }
    out
}

/// How an AI client calls a usable definition: its signature and the
/// `apply_operation` or formula that runs it.
pub fn tool_listing(lib: &Library) -> serde_json::Value {
    use parvu_core::custom::{DefKind, ParamKind, ResultKind};
    use serde_json::json;
    let tools: Vec<_> = lib
        .definitions()
        .iter()
        .filter(|d| d.is_usable())
        .map(|d| {
            let mut args = serde_json::Map::new();
            let mut refs = Vec::new();
            let mut tables = 0;
            for p in &d.params {
                match p.kind {
                    ParamKind::Column => {
                        args.insert(p.name.clone(), json!({ "column": format!("<{} column>", p.name) }));
                        refs.push(format!("[<{} column>]", p.name));
                    }
                    ParamKind::Columns => {
                        let (min, max) = p.column_count();
                        let count = match max {
                            Some(m) if m == min => format!("{min}"),
                            Some(m) => format!("{min} to {m}"),
                            None => format!("{min} or more"),
                        };
                        args.insert(p.name.clone(), json!({ "columns": [format!("<{count} {} columns>", p.name)] }));
                    }
                    ParamKind::Value => {
                        let v = p.default.clone().unwrap_or_else(|| format!("<{}>", p.dtype.as_deref().unwrap_or("VARCHAR")));
                        if !p.optional {
                            refs.push(format!("<{} value>", p.name));
                        }
                        args.insert(p.name.clone(), json!({ "value": v }));
                    }
                    ParamKind::Table => {
                        // The first table is the one the operation is applied to.
                        if tables > 0 {
                            args.insert(p.name.clone(), json!({ "table": "<an open table>" }));
                        }
                        tables += 1;
                    }
                }
            }
            let lists = d.params.iter().any(|p| p.kind == ParamKind::Columns);
            let (kind, usage) = match (d.kind, d.result.kind) {
                (DefKind::Operation, _) => (
                    "operation",
                    json!({ "apply_operation": {
                        "op": "custom_operation", "name": d.name, "args": args,
                        "new_table": d.placement() == parvu_core::custom::Placement::NewTable,
                    } }),
                ),
                // Not callable in formulas: a list of columns can't be passed there.
                (_, ResultKind::Value) if lists => ("summary function", json!({ "note": "takes a list of columns; not usable in formulas" })),
                (_, ResultKind::Value) => (
                    "summary function",
                    json!({
                        "set_formula": format!("{}({}) — one value for the whole column, a column total like SUM", d.name, refs.join(", ")),
                    }),
                ),
                _ if lists => (
                    "row function",
                    json!({
                        "apply_operation": {
                            "op": "custom_function", "name": d.name, "args": args,
                            "output": { "to": "new", "name": "<new column>" },
                        },
                    }),
                ),
                _ => (
                    "row function",
                    json!({
                        "set_formula": format!("{}({})", d.name, refs.join(", ")),
                        "apply_operation": {
                            "op": "custom_function", "name": d.name, "args": args,
                            "output": { "to": "new", "name": "<new column>" },
                        },
                    }),
                ),
            };
            let mut usage = usage;
            // The query tool expands calls of functions without a list of columns.
            if d.kind == DefKind::Function && !lists {
                let sql_args: Vec<String> = d
                    .params
                    .iter()
                    .filter(|p| !(p.kind == ParamKind::Value && p.optional))
                    .map(|p| if p.kind == ParamKind::Value { format!("<{} value>", p.name) } else { format!("<{} column>", p.name) })
                    .collect();
                let call = format!("{}({})", d.name, sql_args.join(", "));
                usage["query"] = json!(if d.result.kind == ResultKind::Value {
                    format!("SELECT <group column>, {call} FROM <table> GROUP BY ALL — or {call} OVER (PARTITION BY …)")
                } else {
                    format!("SELECT {call} FROM <table>")
                });
            }
            if parvu_core::reshape::is_custom_aggregate(d) {
                let values: serde_json::Map<String, serde_json::Value> = d
                    .params
                    .iter()
                    .filter(|p| p.kind == ParamKind::Value)
                    .map(|p| (p.name.clone(), json!(p.default.clone().unwrap_or_else(|| format!("<{}>", p.name)))))
                    .collect();
                usage["group_by_aggregation"] = json!({
                    "column": "<column>", "func": "custom", "custom": { "name": d.name, "values": values },
                });
            }
            let mut v = json!({ "name": d.name, "kind": kind, "signature": d.signature(), "use": usage });
            if !d.title.is_empty() {
                v["title"] = json!(d.title);
            }
            if !d.description.is_empty() {
                v["description"] = json!(d.description);
            }
            if let Some(ex) = d.examples.first() {
                v["example"] = json!({ "input": ex.input, "args": ex.args, "expected": ex.expected });
            }
            v
        })
        .collect();
    json!({ "custom_tools": tools })
}

/// A definition an AI client proposes, read from its JSON. What a receiver
/// computes itself (compiled form, report, status) is dropped.
pub fn proposed_definition(value: serde_json::Value) -> Result<Definition, String> {
    let def: Definition = serde_json::from_value(value).map_err(|e| {
        format!("Not a custom tool definition: {e}. See the propose_custom_tool description for the format.")
    })?;
    let mut def = def.for_export();
    def.sample = false;
    Ok(def)
}

/// Validate a proposed definition against the custom tools in `dir` (none
/// when `None`); `replaces` names the definition it would replace. Nothing
/// is written.
pub fn validate_proposal(dir: Option<&std::path::Path>, def: &Definition, replaces: Option<&str>) -> Report {
    let store = match dir {
        Some(d) => CustomStore::open(d),
        None => CustomStore::open(std::path::Path::new("")),
    };
    validate_in(&store, def, replaces)
}

/// Validate `def` with the names in `store` (except `editing`) taken.
pub fn validate_in(store: &CustomStore, def: &Definition, editing: Option<&str>) -> Report {
    let taken = |n: &str| name_taken(store, editing, n);
    let env = ValidateEnv { name_taken: &taken, app_version: APP_VERSION, expression_sql: Some(&expression_body_sql) };
    validate(def, &env).unwrap_or_else(|e| {
        let mut r = Report::default();
        r.checks.push(parvu_core::custom::Check {
            id: "body".into(),
            status: parvu_core::custom::CheckStatus::Fail,
            message: parvu_core::ops::StepLabel::new("custom.check.internal", &[("error", &e.to_string())]),
            example: None,
        });
        r
    })
}

/// The library of usable definitions in `store`.
pub fn library_of(store: &CustomStore) -> Arc<Library> {
    Arc::new(store.library(APP_VERSION, duckdb_version()))
}

/// Load the store for the headless MCP server: usable definitions only,
/// each validated again, without writing anything.
pub fn headless_library() -> Arc<Library> {
    let mut store = CustomStore::open(&CustomStore::default_dir());
    let snapshot = CustomStore::open(store.dir());
    store.revalidate(APP_VERSION, duckdb_version(), false, &|d| validate_in(&snapshot, d, Some(&d.name)));
    library_of(&store)
}

/// On start, in the background: add the samples once, validate stale
/// definitions again, then hand the library to every window.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        use tauri::Manager;
        let state = app.state::<AppState>();
        let dir = lock(&state.custom).dir().to_path_buf();
        let broken = {
            let mut store = lock(&state.custom);
            let seeded = store.seed_samples(&|d| validate_in(&CustomStore::open(&dir), d, None));
            if seeded > 0 {
                tracing::info!("added {seeded} sample custom tools");
            }
            let snapshot = CustomStore::open(store.dir());
            store.revalidate(APP_VERSION, duckdb_version(), true, &|d| validate_in(&snapshot, d, Some(&d.name)))
        };
        if !broken.is_empty() {
            tracing::warn!("custom tools need attention: {}", broken.join(", "));
        }
        publish(&app, &state);
        if !broken.is_empty() {
            let _ = app.emit("custom-broken", broken);
        }
    });
}

/// Give every window the current library and tell the UIs.
fn publish(app: &AppHandle, state: &AppState) {
    let lib = library_of(&lock(&state.custom));
    *lock(&state.custom_library) = lib.clone();
    let workspaces: Vec<_> = lock(&state.workspaces).values().cloned().collect();
    for ws in workspaces {
        let tabs = {
            let mut ws = lock(&ws);
            ws.set_custom(lib.clone());
            ws.tab_ids()
        };
        for id in refresh_formulas(&ws, &tabs) {
            if let Ok(summary) = lock(&ws).summary(id) {
                crate::commands::remember_formulas(&summary);
            }
        }
    }
    let _ = app.emit("custom-changed", ());
}

/// Tab `id`'s formulas compiled again with the workspace's current custom
/// library, when that changes them: a formula follows the current
/// definition of the custom functions it calls, and one whose function was
/// deleted or broke is kept with its error (an empty column).
fn recompiled_formulas(ws: &Workspace, id: TabId) -> Option<Vec<Formula>> {
    let current = ws.formulas(id).ok()?;
    if current.is_empty() {
        return None;
    }
    let next: Vec<Formula> = current
        .iter()
        .map(|f| {
            crate::commands::compile_tab_formula(ws, id, f.name.clone(), &f.source, f.after.clone())
                .unwrap_or_else(|e| Formula::broken(f.name.clone(), f.source.clone(), f.after.clone(), e.message))
        })
        .collect();
    (next != current).then_some(next)
}

/// [`recompiled_formulas`] for each of `tabs`, put in place. Returns the
/// tabs whose formulas changed.
///
/// Compiling is quick and done holding the workspace; a total (`SUM([x])`
/// in a formula) reads the whole table, so the totals are computed on a
/// second connection without holding it, and paging stays responsive
/// (AGENTS.md pitfall 5). Only a tab whose formulas compile differently is
/// rebuilt. A tab that changed meanwhile is compiled again.
pub fn refresh_formulas(ws: &Mutex<Workspace>, tabs: &[TabId]) -> Vec<TabId> {
    let mut changed = Vec::new();
    for &id in tabs {
        for _attempt in 0..3 {
            let job = {
                let w = lock(ws);
                let Some(next) = recompiled_formulas(&w, id) else { break };
                match w.formula_totals_job(id, next) {
                    Ok(job) => job,
                    Err(e) => {
                        tracing::warn!("formulas of tab {id} not updated after a custom tool changed: {e}");
                        break;
                    }
                }
            };
            let resolved = match job.resolve() {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("formulas of tab {id} not updated after a custom tool changed: {e}");
                    break;
                }
            };
            let placed = lock(ws).set_resolved_formulas(id, &job, &resolved);
            match placed {
                Ok((true, recount)) => {
                    if let Some(query) = recount {
                        match job.count(&query) {
                            Ok(n) => {
                                let _ = lock(ws).set_counted_rows(id, &query, n);
                            }
                            Err(e) => tracing::warn!("rows of tab {id} not counted again: {e}"),
                        }
                    }
                    changed.push(id);
                    break;
                }
                // The tab changed meanwhile: compile again.
                Ok((false, _)) => continue,
                Err(e) => {
                    tracing::warn!("formulas of tab {id} not updated after a custom tool changed: {e}");
                    break;
                }
            }
        }
    }
    changed
}

async fn publish_async(app: AppHandle) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<AppState>();
        publish(&app, &state);
    })
    .await;
}

fn io(e: std::io::Error) -> ApiError {
    ApiError::new("io", e.to_string())
}

/// What the manager and the menus show.
#[derive(Serialize)]
pub struct Listing {
    pub entries: Vec<Entry>,
    /// Type set names a signature may use.
    pub type_sets: Vec<&'static str>,
    /// DuckDB types a value parameter may have.
    pub value_types: Vec<&'static str>,
    pub folder: PathBuf,
    /// The usable custom functions as the expression editor and the
    /// formula bar complete them.
    pub functions: Vec<FunctionDef>,
}

#[tauri::command(rename_all = "snake_case")]
pub fn custom_list(state: State<'_, AppState>) -> Listing {
    let store = lock(&state.custom);
    Listing {
        entries: store.entries().to_vec(),
        type_sets: parvu_core::custom::signature::TYPE_SETS.iter().map(|s| s.name).collect(),
        value_types: parvu_core::custom::signature::VALUE_TYPES.to_vec(),
        folder: store.dir().to_path_buf(),
        functions: custom_functions(&lock(&state.custom_library)),
    }
}

/// Run every check without saving. `editing` is the name the definition
/// had when the editor opened (it may keep it).
#[tauri::command(rename_all = "snake_case")]
pub async fn custom_validate(state: State<'_, AppState>, definition: Definition, editing: Option<String>) -> ApiResult<Report> {
    let dir = lock(&state.custom).dir().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = CustomStore::open(&dir);
        Ok(validate_in(&snapshot, &definition, editing.as_deref()))
    })
    .await
    .map_err(|e| ApiError::new("app", e.to_string()))?
}

#[derive(Serialize)]
pub struct Saved {
    pub saved: bool,
    pub status: Status,
    pub report: Report,
}

/// Validate and save. A definition that does not pass is saved only as a
/// draft (`draft: true`); otherwise nothing is written.
#[tauri::command(rename_all = "snake_case")]
pub async fn custom_save(
    app: AppHandle,
    state: State<'_, AppState>,
    definition: Definition,
    editing: Option<String>,
    draft: bool,
) -> ApiResult<Saved> {
    {
        let store = lock(&state.custom);
        let renamed = editing.as_deref().is_none_or(|e| !e.eq_ignore_ascii_case(&definition.name));
        if renamed && store.get(&definition.name).is_some() {
            return Err(ApiError::new("custom", format!("A custom tool named {} already exists", definition.name)));
        }
    }
    let dir = lock(&state.custom).dir().to_path_buf();
    let ed = editing.clone();
    let def = definition.clone();
    let report = tauri::async_runtime::spawn_blocking(move || validate_in(&CustomStore::open(&dir), &def, ed.as_deref()))
        .await
        .map_err(|e| ApiError::new("app", e.to_string()))?;
    let passed = report.passed();
    if !passed && !draft {
        return Ok(Saved { saved: false, status: Status::Draft, report });
    }
    let mut def = checked(definition, report.clone(), true);
    if draft && passed {
        // Kept as a draft on purpose: not offered in menus yet.
        def.status = Status::Draft;
        def.report = Some(report.clone());
    }
    let status = def.status;
    lock(&state.custom).save(def, editing.as_deref()).map_err(io)?;
    publish_async(app).await;
    Ok(Saved { saved: true, status, report })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn custom_delete(app: AppHandle, state: State<'_, AppState>, name: String) -> ApiResult<()> {
    lock(&state.custom).delete(&name).map_err(io)?;
    publish_async(app).await;
    Ok(())
}

/// Add the samples whose names are free. Returns how many were added.
#[tauri::command(rename_all = "snake_case")]
pub async fn custom_restore_samples(app: AppHandle, state: State<'_, AppState>) -> ApiResult<usize> {
    let dir = lock(&state.custom).dir().to_path_buf();
    let n = tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<AppState>();
        let n = lock(&state.custom).restore_samples(&|d| validate_in(&CustomStore::open(&dir), d, None));
        publish(&app, &state);
        n
    })
    .await
    .map_err(|e| ApiError::new("app", e.to_string()))?;
    Ok(n)
}

/// Write `names` (all when empty) to a `.json` file. Returns how many.
#[tauri::command(rename_all = "snake_case")]
pub fn custom_export(state: State<'_, AppState>, names: Vec<String>, path: PathBuf) -> ApiResult<usize> {
    lock(&state.custom).export(&names, &path).map_err(io)
}

/// A definition in a file to import.
#[derive(Serialize)]
pub struct ImportItem {
    pub name: String,
    pub kind: parvu_core::custom::DefKind,
    pub title: String,
    /// A definition of this name exists.
    pub exists: bool,
}

/// What a file holds, before importing it.
#[tauri::command(rename_all = "snake_case")]
pub fn custom_import_preview(state: State<'_, AppState>, path: PathBuf) -> ApiResult<Vec<ImportItem>> {
    let text = std::fs::read_to_string(&path)?;
    let bundle = Bundle::parse(&text)?;
    let store = lock(&state.custom);
    Ok(bundle
        .definitions
        .into_iter()
        .map(|d| ImportItem { exists: store.get(&d.name).is_some(), name: d.name, kind: d.kind, title: d.title })
        .collect())
}

/// Import a file; `decisions` says what to do with each name that exists.
#[tauri::command(rename_all = "snake_case")]
pub async fn custom_import(
    app: AppHandle,
    path: PathBuf,
    decisions: BTreeMap<String, Clash>,
) -> ApiResult<ImportResult> {
    let text = std::fs::read_to_string(&path)?;
    let bundle = Bundle::parse(&text)?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<AppState>();
        let r = lock(&state.custom).import(bundle, &decisions, &|d, store| validate_in(store, d, None));
        publish(&app, &state);
        r
    })
    .await
    .map_err(|e| ApiError::new("app", e.to_string()))?;
    result.map_err(io)
}

/// A summary function's value over the tab (a job: it reads every row).
#[tauri::command(rename_all = "snake_case")]
pub async fn custom_value(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    name: String,
    args: BTreeMap<String, CustomArg>,
    job: u64,
) -> ApiResult<Option<String>> {
    run_job(state, window, job, "custom", Some(id), move |ws| ws.custom_value_input(id, &name, &args), |ctx, (items, source, alias)| {
        Ok(parvu_core::sql::query_whole_table(ctx.conn, items, source, alias, |r| r.get::<_, Option<String>>(0))?)
    })
    .await
}

/// The first rows an operation would give (a job).
#[tauri::command(rename_all = "snake_case")]
pub async fn custom_preview(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    op: Operation,
    rows: usize,
    job: u64,
) -> ApiResult<parvu_core::custom::ExampleTable> {
    let n = rows.clamp(1, 200);
    run_job(state, window, job, "custom-preview", Some(id), move |ws| ws.op_sql(id, &op), move |ctx, sql| {
        parvu_core::custom::examples::run_sql(ctx.conn, &format!("SELECT * FROM ({sql}) LIMIT {n}"))
    })
    .await
}

/// A free name for a new definition: `name`, or `name_2`, `name_3`…
fn free_name(store: &CustomStore, name: &str) -> String {
    let taken = |n: &str| name_taken(store, None, n).is_some();
    if !taken(name) {
        return name.to_string();
    }
    (2..).map(|i| format!("{name}_{i}")).find(|n| !taken(n)).unwrap_or_default()
}

/// "Save steps as custom operation…": tab `id`'s steps as a draft custom
/// operation, the input columns in `params` becoming column parameters,
/// with an example made of its first rows. Nothing is saved: the window
/// opens the editor with it. The workspace is held only to read the steps
/// and the first rows; the rest runs on a small database of its own.
#[tauri::command(rename_all = "snake_case")]
pub async fn custom_from_steps(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    params: Vec<String>,
) -> ApiResult<parvu_core::custom::StepsDraft> {
    let dir = lock(&state.custom).dir().to_path_buf();
    let capture = crate::commands::with_ws(state, window, move |ws| Ok(ws.steps_capture(id)?)).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut draft = parvu_core::custom::record::draft(&capture, &params)?;
        if let Some(def) = &mut draft.definition {
            def.name = free_name(&CustomStore::open(&dir), &def.name);
        }
        Ok(draft)
    })
    .await
    .map_err(|e| ApiError::new("app", e.to_string()))?
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// The samples, validated and compiled, as a library.
    pub fn sample_library() -> Arc<Library> {
        let store = CustomStore::open(std::path::Path::new(""));
        let defs = parvu_core::custom::samples().into_iter().map(|d| {
            let r = validate_in(&store, &d, None);
            checked(d, r, true)
        });
        Arc::new(Library::new(defs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expression_and_excel_names_are_taken() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = CustomStore::open(dir.path());
        assert_eq!(name_taken(&store, None, "round").as_deref(), Some("custom.taken.expression"));
        assert_eq!(name_taken(&store, None, "drop_null").as_deref(), Some("custom.taken.expression"));
        assert_eq!(name_taken(&store, None, "vlookup").as_deref(), Some("custom.taken.expression"));
        assert!(name_taken(&store, None, "clean_phone").is_none());
    }

    #[test]
    fn samples_pass_with_the_app_names_taken() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = CustomStore::open(dir.path());
        for d in parvu_core::custom::samples() {
            let r = validate_in(&store, &d, None);
            assert!(r.passed(), "{}: {:?}", d.name, r.first_failure());
        }
    }

    #[test]
    fn a_custom_operation_runs_through_the_mcp_tool_schema() {
        // apply_operation takes an Operation; its schema lists the custom variants.
        let schema = parvu_core::ops::operation_schema().to_string();
        assert!(schema.contains("custom_operation") && schema.contains("custom_function"));
    }

    use super::test_support::sample_library;

    #[test]
    fn columns_parameters_are_listed_for_ai_clients() {
        let lib = sample_library();
        let listing = tool_listing(&lib);
        let tools = listing["custom_tools"].as_array().unwrap();
        let d = tools.iter().find(|t| t["name"] == "dedupe_by").unwrap();
        assert_eq!(d["use"]["apply_operation"]["args"]["keys"], serde_json::json!({ "columns": ["<1 or more keys columns>"] }));
        assert!(d["signature"].as_str().unwrap().contains("keys: columns<any>"));
        assert_eq!(d["example"]["args"]["keys"], serde_json::json!(["customer"]));
        let p = tools.iter().find(|t| t["name"] == "percentile_value").unwrap();
        assert_eq!(p["use"]["group_by_aggregation"]["custom"], serde_json::json!({ "name": "percentile_value", "values": { "p": "0.9" } }));
        let w = tools.iter().find(|t| t["name"] == "weighted_average").unwrap();
        assert!(w["use"].get("group_by_aggregation").is_none(), "two columns");
        // Not callable in formulas: not in the expression language's registry.
        let mut join = (*lib.get("title_case").unwrap()).clone();
        join.name = "join_text".into();
        join.params[0].kind = parvu_core::custom::ParamKind::Columns;
        assert!(function_def(&join).is_none());
        // The operation schema takes a list of columns.
        let op: Operation = serde_json::from_value(serde_json::json!({
            "op": "custom_operation", "name": "dedupe_by",
            "args": { "keys": { "columns": ["a", "b"] }, "latest": { "column": "c" } }
        }))
        .unwrap();
        assert!(matches!(op, Operation::CustomOperation { ref args, .. } if args["keys"] == CustomArg::Columns(vec!["a".into(), "b".into()])));
    }

    fn sales() -> (tempfile::TempDir, parvu_core::Workspace, TabId) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("sales.csv");
        std::fs::write(&path, "phone,price,qty\n+1 (555) 010-2030,10,1\n555.0102,20,1\nn/a,30,2\n").unwrap();
        let mut ws = parvu_core::Workspace::new(100, None).unwrap();
        let id = ws.open(&path).unwrap();
        (dir, ws, id)
    }

    fn column(ws: &parvu_core::Workspace, id: TabId, name: &str) -> Vec<Option<String>> {
        let page = ws.page(id, 1).unwrap();
        let i = page.columns.iter().position(|c| c.name == name).unwrap();
        page.rows.into_iter().map(|r| r[i].clone()).collect()
    }

    #[test]
    fn custom_functions_join_the_expression_language() {
        let lib = sample_library();
        let reg = registry_for(&lib);
        assert!(Arc::ptr_eq(&reg, &registry_for(&lib)), "built once per library");
        let f = reg.lookup("clean_phone").unwrap();
        assert!(f.custom && f.arg_slots && !f.aggregate);
        assert_eq!(f.params[0].accepts, Some(vec![LogicalType::Text]));
        assert_eq!(f.params[1].default.as_deref(), Some("'+'"));
        assert!(f.sql_template.as_deref().unwrap().contains("CAST(__pv_arg_1 AS VARCHAR)"));
        assert!(reg.lookup("weighted_average").unwrap().aggregate);
        // Operations are not functions.
        assert!(reg.lookup("top_n_per_group").is_none());
        assert!(reg.lookup("SUM").is_some(), "the built-ins stay");
    }

    #[test]
    fn custom_functions_run_in_expressions_and_formulas() {
        let (_d, mut ws, id) = sales();
        ws.set_custom(sample_library());
        // A row function in a formula, with and without its optional value.
        for (name, src, want) in [
            ("clean", "= clean_phone([phone])", [Some("+15550102030"), Some("+5550102"), None]),
            ("clean00", "= clean_phone([phone], \"00\")", [Some("0015550102030"), Some("005550102"), None]),
        ] {
            let f = crate::commands::compile_tab_formula(&ws, id, name.into(), src, None).unwrap();
            ws.set_formula(id, f).unwrap();
            assert_eq!(column(&ws, id, name), want.map(|v| v.map(str::to_string)));
        }
        // A summary function is a total: (10·1 + 20·1 + 30·2) / 4 = 22.5.
        let f = crate::commands::compile_tab_formula(&ws, id, "gap".into(), "= [price] - weighted_average([price], [qty])", None).unwrap();
        assert_eq!(f.totals.len(), 1);
        ws.set_formula(id, f).unwrap();
        assert_eq!(column(&ws, id, "gap"), [Some("-12.5".into()), Some("-2.5".into()), Some("7.5".into())]);
        // A column of the wrong type is refused before DuckDB sees it.
        let e = crate::commands::compile_tab_formula(&ws, id, "x".into(), "= clean_phone([price])", None).unwrap_err();
        assert_eq!(e.message, "clean_phone: phone takes text, but this argument is a number");
        // The expression editor sees them too.
        let catalog = crate::commands::catalog_of(&ws);
        let t = parvu_dsl::translate("sales[m] = clean_phone(sales[phone])", &catalog, &registry(&ws), "sales").unwrap();
        assert!(t.sql.contains("regexp_replace((sales.phone)"), "{}", t.sql);
    }

    #[test]
    fn a_formula_whose_function_was_deleted_is_kept_with_an_error() {
        let (_d, mut ws, id) = sales();
        let lib = sample_library();
        ws.set_custom(lib.clone());
        let f = crate::commands::compile_tab_formula(&ws, id, "gap".into(), "= [price] - weighted_average([price], [qty])", None).unwrap();
        ws.set_formula(id, f).unwrap();
        let f = crate::commands::compile_tab_formula(&ws, id, "twice".into(), "= [price] * 2", None).unwrap();
        ws.set_formula(id, f).unwrap();
        let ws = Mutex::new(ws);
        let refresh = |ws: &Mutex<Workspace>| refresh_formulas(ws, &[id]);
        assert!(refresh(&ws).is_empty(), "nothing changed, nothing rebuilt");
        // Delete weighted_average.
        let rest = lib.definitions().into_iter().filter(|d| d.name != "weighted_average").map(|d| (*d).clone());
        lock(&ws).set_custom(Arc::new(Library::new(rest)));
        assert_eq!(refresh(&ws), [id]);
        {
            let ws = lock(&ws);
            let formulas = ws.formulas(id).unwrap().to_vec();
            assert_eq!(formulas.len(), 2, "kept, not dropped");
            assert_eq!(formulas[0].source, "[price] - weighted_average([price], [qty])");
            assert!(formulas[0].error.as_deref().unwrap().contains("weighted_average"), "{:?}", formulas[0].error);
            assert_eq!(column(&ws, id, "gap"), [None, None, None]);
            assert_eq!(column(&ws, id, "twice"), [Some("20".into()), Some("40".into()), Some("60".into())]);
        }
        // It comes back with the function; its total is computed off the workspace.
        lock(&ws).set_custom(lib.clone());
        assert_eq!(refresh(&ws), [id]);
        {
            let mut w = lock(&ws);
            assert!(w.formulas(id).unwrap()[0].error.is_none());
            assert_eq!(column(&w, id, "gap")[0].as_deref(), Some("-12.5"));
            // A step that filters on the formula: its rows are counted again.
            w.run_sql(id, "SELECT * FROM sales WHERE gap IS NOT NULL", None).unwrap();
            assert_eq!(w.summary(id).unwrap().total_rows, 3);
            let rest = lib.definitions().into_iter().filter(|d| d.name != "weighted_average").map(|d| (*d).clone());
            w.set_custom(Arc::new(Library::new(rest)));
        }
        assert_eq!(refresh(&ws), [id]);
        assert_eq!(lock(&ws).summary(id).unwrap().total_rows, 0, "gap is empty now");
        // Totals are kept as markers, so a later rebuild computes them again.
        lock(&ws).set_custom(lib);
        assert_eq!(refresh(&ws), [id]);
        assert_eq!(lock(&ws).summary(id).unwrap().total_rows, 3);
        assert_eq!(lock(&ws).formulas(id).unwrap()[0].totals.len(), 1);
    }

    #[test]
    fn expression_bodies_compile_and_validate() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = CustomStore::open(dir.path());
        let mut d = parvu_core::custom::samples().into_iter().find(|d| d.name == "weighted_average").unwrap();
        d.name = "weighted_mean".into();
        d.body = parvu_core::custom::Body {
            language: parvu_core::custom::BodyLanguage::Expression,
            source: "SUM([x] * [w]) / SUM(IF(ISBLANK([x]), NULL, [w]))".into(),
        };
        let sql = expression_body_sql(&d).unwrap();
        assert!(sql.starts_with("SUM(x * w) / SUM(IF("), "{sql}");
        let r = validate_in(&store, &d, None);
        assert!(r.passed(), "{:?}", r.first_failure());
        // Errors from the expression language are the body's.
        d.body.source = "[nope] + 1".into();
        let r = validate_in(&store, &d, None);
        let f = r.first_failure().unwrap();
        assert_eq!((f.id.as_str(), f.message.key.as_str()), ("body", "custom.check.expression_error"));
        d.body.source = "drop_null(__pv_args[x])".into();
        assert_eq!(validate_in(&store, &d, None).first_failure().unwrap().message.key, "custom.check.body_reserved");
        // A row function in the expression language.
        let mut t = parvu_core::custom::samples().into_iter().find(|d| d.name == "title_case").unwrap();
        t.name = "shout".into();
        t.body = parvu_core::custom::Body { language: parvu_core::custom::BodyLanguage::Expression, source: "UPPER([text])".into() };
        t.examples[0].expected.rows = t.examples[0].input.rows.iter().map(|r| vec![r[0].as_ref().map(|v| v.to_uppercase())]).collect();
        let r = validate_in(&store, &t, None);
        assert!(r.passed(), "{:?}", r.first_failure());
    }
}
