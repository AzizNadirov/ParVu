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
            ParamKind::Table => return None,
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
    let fail = |e: &dyn std::fmt::Display| Issue::new("custom.check.parse", &[("error", &e.to_string())]);
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
        let mut ws = lock(&ws);
        ws.set_custom(lib.clone());
        for id in refresh_formulas(&mut ws) {
            if let Ok(summary) = ws.summary(id) {
                crate::commands::remember_formulas(&summary);
            }
        }
    }
    let _ = app.emit("custom-changed", ());
}

/// Compile the formulas of every tab again with the workspace's current
/// custom library: a formula follows the current definition of the custom
/// functions it calls, and one whose function was deleted or broke is kept
/// with its error (an empty column). Returns the tabs whose formulas changed.
///
/// Only a tab whose formulas compile differently is rebuilt, so this costs
/// nothing for the tabs that don't use the changed functions.
pub fn refresh_formulas(ws: &mut Workspace) -> Vec<TabId> {
    let mut changed = Vec::new();
    for id in ws.tab_ids() {
        let Ok(current) = ws.formulas(id).map(<[Formula]>::to_vec) else { continue };
        if current.is_empty() {
            continue;
        }
        let next: Vec<Formula> = current
            .iter()
            .map(|f| {
                crate::commands::compile_tab_formula(ws, id, f.name.clone(), &f.source, f.after.clone())
                    .unwrap_or_else(|e| Formula::broken(f.name.clone(), f.source.clone(), f.after.clone(), e.message))
            })
            .collect();
        if next == current {
            continue;
        }
        match ws.restore_formulas(id, next) {
            Ok(()) => changed.push(id),
            Err(e) => tracing::warn!("formulas of tab {id} not updated after a custom tool changed: {e}"),
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

    /// The samples, validated and compiled, as a library.
    fn sample_library() -> Arc<Library> {
        let dir = tempfile::TempDir::new().unwrap();
        let store = CustomStore::open(dir.path());
        let defs = parvu_core::custom::samples().into_iter().map(|d| {
            let r = validate_in(&store, &d, None);
            checked(d, r, true)
        });
        Arc::new(Library::new(defs))
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
        assert!(refresh_formulas(&mut ws).is_empty(), "nothing changed, nothing rebuilt");
        // Delete weighted_average.
        let rest = lib.definitions().into_iter().filter(|d| d.name != "weighted_average").map(|d| (*d).clone());
        ws.set_custom(Arc::new(Library::new(rest)));
        assert_eq!(refresh_formulas(&mut ws), [id]);
        let formulas = ws.formulas(id).unwrap().to_vec();
        assert_eq!(formulas.len(), 2, "kept, not dropped");
        assert_eq!(formulas[0].source, "[price] - weighted_average([price], [qty])");
        assert!(formulas[0].error.as_deref().unwrap().contains("weighted_average"), "{:?}", formulas[0].error);
        assert_eq!(column(&ws, id, "gap"), [None, None, None]);
        assert_eq!(column(&ws, id, "twice"), [Some("20".into()), Some("40".into()), Some("60".into())]);
        // It comes back with the function.
        ws.set_custom(lib);
        assert_eq!(refresh_formulas(&mut ws), [id]);
        assert!(ws.formulas(id).unwrap()[0].error.is_none());
        assert_eq!(column(&ws, id, "gap")[0].as_deref(), Some("-12.5"));
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
        assert_eq!((f.id.as_str(), f.message.key.as_str()), ("body", "custom.check.parse"));
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
