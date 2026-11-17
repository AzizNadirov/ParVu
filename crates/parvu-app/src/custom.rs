//! Commands for custom functions and operations (plans/CUSTOM_TOOLS_PLAN.md
//! §7, §8): list, validate, save, delete, import, export, and running a
//! summary function or a preview as a job.
//!
//! Validation runs on the blocking pool on its own small DuckDB, never
//! under a workspace lock. A change updates every window's workspace and
//! emits `custom-changed`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use parvu_core::TabId;
use parvu_core::custom::{Bundle, CustomArg, Definition, Library, Report, Status, ValidateEnv, validate};
use parvu_core::ops::Operation;
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

/// Validate `def` with the names in `store` (except `editing`) taken.
pub fn validate_in(store: &CustomStore, def: &Definition, editing: Option<&str>) -> Report {
    let taken = |n: &str| name_taken(store, editing, n);
    let env = ValidateEnv { name_taken: &taken, app_version: APP_VERSION };
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
        lock(&ws).set_custom(lib.clone());
    }
    let _ = app.emit("custom-changed", ());
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
}

#[tauri::command(rename_all = "snake_case")]
pub fn custom_list(state: State<'_, AppState>) -> Listing {
    let store = lock(&state.custom);
    Listing {
        entries: store.entries().to_vec(),
        type_sets: parvu_core::custom::signature::TYPE_SETS.iter().map(|s| s.name).collect(),
        value_types: parvu_core::custom::signature::VALUE_TYPES.to_vec(),
        folder: store.dir().to_path_buf(),
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
}
