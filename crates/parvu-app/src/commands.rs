//! Tauri commands: the whole interface between the UI and the Rust crates.
//!
//! Commands that touch data run on the blocking thread pool so the UI thread
//! never waits on DuckDB. Each window has its own workspace, found by the
//! calling window's label.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use parvu_config::{I18n, Language, Settings};
use parvu_core::export::{CAST_TYPES, ExportOptions, PARQUET_COMPRESSIONS};
use parvu_core::ops::{Operation, StepLabel};
use parvu_core::page::{Cell, ColumnMeta, Page};
use parvu_core::search::{SearchHit, SearchOptions};
use parvu_core::stats::{ColumnStats, SampleMode};
use parvu_core::{TabId, TabSummary, Workspace};
use parvu_dsl::{Catalog, FunctionDef, FunctionRegistry, TranslationKind};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::error::{ApiError, ApiResult};
use crate::state::{AppState, ExportJob, lock};

type Ws = Arc<Mutex<Workspace>>;

fn ws_of(state: &AppState, window: &WebviewWindow) -> ApiResult<Ws> {
    state.workspace(window.label())
}

/// Run `f` with the window's workspace on the blocking pool.
async fn with_ws<T, F>(state: State<'_, AppState>, window: WebviewWindow, f: F) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce(&mut Workspace) -> ApiResult<T> + Send + 'static,
{
    let ws = ws_of(&state, &window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = lock(&ws);
        f(&mut guard)
    })
    .await
    .map_err(|e| ApiError::new("app", e.to_string()))?
}

fn short(text: &str, max: usize) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() > max {
        format!("{}…", one_line.chars().take(max).collect::<String>())
    } else {
        one_line
    }
}

// ---------------------------------------------------------------- bootstrap

#[derive(Serialize)]
pub struct ThemeInfo {
    pub name: String,
    pub css: String,
}

#[derive(Serialize)]
pub struct Bootstrap {
    pub version: &'static str,
    pub window_label: String,
    pub settings: Settings,
    pub language: String,
    pub languages: Vec<Language>,
    pub translations: BTreeMap<String, String>,
    pub english: BTreeMap<String, String>,
    pub theme: ThemeInfo,
    pub themes: Vec<String>,
    pub recents: Vec<String>,
    pub extensions: Vec<&'static str>,
    pub cast_types: Vec<&'static str>,
    pub parquet_compressions: Vec<&'static str>,
    pub functions: Vec<FunctionDef>,
    pub methods: BTreeMap<String, Vec<String>>,
    pub crash_report: Option<String>,
    pub initial_files: Vec<PathBuf>,
    pub log_dir: PathBuf,
}

fn methods_by_type() -> BTreeMap<String, Vec<String>> {
    parvu_dsl::LogicalType::ALL
        .iter()
        .map(|t| (t.name().to_string(), parvu_dsl::methods_for_type(*t).iter().map(|m| m.to_string()).collect()))
        .filter(|(_, v): &(String, Vec<String>)| !v.is_empty())
        .collect()
}

#[tauri::command(rename_all = "snake_case")]
pub fn bootstrap(state: State<'_, AppState>, window: WebviewWindow) -> ApiResult<Bootstrap> {
    let cfg = lock(&state.config);
    let lang = cfg.i18n.current_language().to_string();
    let theme = cfg.themes.current_theme();
    let is_first = window.label() == "main";
    Ok(Bootstrap {
        version: env!("CARGO_PKG_VERSION"),
        window_label: window.label().to_string(),
        settings: cfg.settings.settings().clone(),
        translations: I18n::catalog(&lang).clone(),
        english: I18n::catalog("en").clone(),
        language: lang,
        languages: I18n::available_languages().to_vec(),
        theme: ThemeInfo { name: theme.name.clone(), css: theme.css_variables() },
        themes: cfg.themes.list_themes(),
        recents: cfg.recents.recents().to_vec(),
        extensions: parvu_core::format::supported_extensions(),
        cast_types: CAST_TYPES.to_vec(),
        parquet_compressions: PARQUET_COMPRESSIONS.to_vec(),
        functions: FunctionRegistry::default().all_functions().into_iter().cloned().collect(),
        methods: methods_by_type(),
        crash_report: if is_first { lock(&state.pending_crash).take() } else { None },
        initial_files: if is_first { std::mem::take(&mut *lock(&state.initial_files)) } else { Vec::new() },
        log_dir: parvu_config::paths::log_dir().unwrap_or_default(),
    })
}

// ---------------------------------------------------------------- settings, themes, language

#[tauri::command(rename_all = "snake_case")]
pub fn update_settings(
    state: State<'_, AppState>,
    values: serde_json::Map<String, serde_json::Value>,
) -> ApiResult<Settings> {
    let mut cfg = lock(&state.config);
    let rejected = cfg.settings.update(values)?;
    if !rejected.is_empty() {
        tracing::warn!("rejected settings: {rejected:?}");
    }
    let page_size = cfg.settings.settings().rows_per_page();
    let memory_mb = cfg.settings.settings().query_memory_limit_mb.max(0) as u64;
    drop(cfg);
    let env_limit = std::env::var_os("PARVU_MEMORY_LIMIT_MB").is_some();
    for ws in lock(&state.workspaces).values() {
        let mut ws = lock(ws);
        ws.set_page_size(page_size);
        if !env_limit {
            ws.set_memory_limit_mb(memory_mb)?;
        }
    }
    Ok(lock(&state.config).settings.settings().clone())
}

#[tauri::command(rename_all = "snake_case")]
pub fn reset_settings(state: State<'_, AppState>) -> ApiResult<Settings> {
    let mut cfg = lock(&state.config);
    let paths = cfg.settings.paths().clone();
    parvu_config::SettingsManager::reset_with(&paths, parvu_config::DefaultsSource::Bundled)?;
    cfg.settings = parvu_config::SettingsManager::with_paths(paths);
    Ok(cfg.settings.settings().clone())
}

#[derive(Serialize)]
pub struct LanguageChange {
    pub language: String,
    pub translations: BTreeMap<String, String>,
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_language(app: AppHandle, state: State<'_, AppState>, code: String) -> ApiResult<LanguageChange> {
    let mut cfg = lock(&state.config);
    if !cfg.i18n.set_language(&code) {
        return Err(ApiError::new("config", format!("Unknown language: {code}")));
    }
    let mut values = serde_json::Map::new();
    values.insert("current_language".into(), code.clone().into());
    cfg.settings.update(values)?;
    let change = LanguageChange { language: code.clone(), translations: I18n::catalog(&code).clone() };
    drop(cfg);
    let _ = app.emit("language-changed", &code);
    Ok(change)
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_theme(app: AppHandle, state: State<'_, AppState>, name: String) -> ApiResult<ThemeInfo> {
    let mut cfg = lock(&state.config);
    if !cfg.themes.set_theme(&name) {
        return Err(ApiError::new("config", format!("Unknown theme: {name}")));
    }
    let mut values = serde_json::Map::new();
    values.insert("current_theme".into(), name.clone().into());
    cfg.settings.update(values)?;
    let info = ThemeInfo { name: name.clone(), css: cfg.themes.current_theme().css_variables() };
    drop(cfg);
    let _ = app.emit("theme-changed", &name);
    Ok(info)
}

/// CSS variables of a theme without switching to it (theme preview).
#[tauri::command(rename_all = "snake_case")]
pub fn theme_css(state: State<'_, AppState>, name: String) -> ApiResult<String> {
    let cfg = lock(&state.config);
    cfg.themes
        .get_theme(&name)
        .map(|t| t.css_variables())
        .ok_or_else(|| ApiError::new("config", format!("Unknown theme: {name}")))
}

#[derive(Serialize)]
pub struct ThemeDetails {
    pub name: String,
    pub description: String,
    pub author: String,
    pub builtin: bool,
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_themes(state: State<'_, AppState>) -> Vec<ThemeDetails> {
    let cfg = lock(&state.config);
    cfg.themes
        .list_themes()
        .into_iter()
        .filter_map(|n| cfg.themes.get_theme(&n))
        .map(|t| ThemeDetails {
            builtin: cfg.themes.is_builtin(&t.name),
            name: t.name,
            description: t.description,
            author: t.author,
        })
        .collect()
}

#[tauri::command(rename_all = "snake_case")]
pub fn import_theme(state: State<'_, AppState>, path: PathBuf) -> ApiResult<String> {
    Ok(lock(&state.config).themes.import_theme(&path)?)
}

#[tauri::command(rename_all = "snake_case")]
pub fn export_theme(state: State<'_, AppState>, name: String, path: PathBuf) -> ApiResult<()> {
    Ok(lock(&state.config).themes.export_theme(&name, &path)?)
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_theme(state: State<'_, AppState>, name: String) -> ApiResult<()> {
    Ok(lock(&state.config).themes.delete_theme(&name)?)
}

// ---------------------------------------------------------------- recents & windows

#[tauri::command(rename_all = "snake_case")]
pub fn recents(state: State<'_, AppState>) -> Vec<String> {
    lock(&state.config).recents.recents().to_vec()
}

#[tauri::command(rename_all = "snake_case")]
pub fn clear_recents(app: AppHandle, state: State<'_, AppState>) -> ApiResult<()> {
    let mut cfg = lock(&state.config);
    cfg.recents.clear();
    cfg.recents.save()?;
    let _ = app.emit("recents-changed", Vec::<String>::new());
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn new_window(app: AppHandle, state: State<'_, AppState>) -> ApiResult<String> {
    let n = state.next_window.fetch_add(1, Ordering::Relaxed);
    let label = format!("window-{n}");
    crate::build_window(&app, &label)?;
    Ok(label)
}

// ---------------------------------------------------------------- tabs

#[tauri::command(rename_all = "snake_case")]
pub async fn open_file(
    app: AppHandle,
    state: State<'_, AppState>,
    window: WebviewWindow,
    path: PathBuf,
) -> ApiResult<TabSummary> {
    let p = path.clone();
    let summary = with_ws(state.clone(), window, move |ws| {
        let id = ws.open(&p)?;
        Ok(ws.summary(id)?)
    })
    .await?;
    let recents = {
        let mut cfg = lock(&state.config);
        let s = cfg.settings.settings().clone();
        if cfg.recents.add_if_enabled(path.to_string_lossy().to_string(), &s) {
            cfg.recents.save()?;
        }
        cfg.recents.recents().to_vec()
    };
    let _ = app.emit("recents-changed", recents);
    Ok(summary)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn close_tab(state: State<'_, AppState>, window: WebviewWindow, id: TabId) -> ApiResult<()> {
    with_ws(state, window, move |ws| Ok(ws.close(id)?)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn tabs(state: State<'_, AppState>, window: WebviewWindow) -> ApiResult<Vec<TabSummary>> {
    with_ws(state, window, |ws| Ok(ws.summaries())).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn tab_summary(state: State<'_, AppState>, window: WebviewWindow, id: TabId) -> ApiResult<TabSummary> {
    with_ws(state, window, move |ws| Ok(ws.summary(id)?)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_page(state: State<'_, AppState>, window: WebviewWindow, id: TabId, page: u64) -> ApiResult<Page> {
    with_ws(state, window, move |ws| Ok(ws.page(id, page)?)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn columns(state: State<'_, AppState>, window: WebviewWindow, id: TabId) -> ApiResult<Vec<ColumnMeta>> {
    with_ws(state, window, move |ws| Ok(ws.columns(id)?)).await
}

// ---------------------------------------------------------------- queries and operations

/// Run SQL typed in the SQL editor. The tab's name refers to the original file.
#[tauri::command(rename_all = "snake_case")]
pub async fn run_sql(state: State<'_, AppState>, window: WebviewWindow, id: TabId, sql: String) -> ApiResult<TabSummary> {
    with_ws(state, window, move |ws| {
        ws.run_sql(id, &sql, None)?;
        Ok(ws.summary(id)?)
    })
    .await
}

fn catalog_of(ws: &Workspace) -> Catalog {
    let mut cat = Catalog::new();
    for (name, cols) in ws.catalog() {
        cat.register_table(&name, cols.into_iter().map(|c| (c.name, c.dtype)));
    }
    cat
}

/// Every open table with its current columns, for editor autocomplete.
#[tauri::command(rename_all = "snake_case")]
pub async fn catalog(
    state: State<'_, AppState>,
    window: WebviewWindow,
) -> ApiResult<Vec<(String, Vec<ColumnMeta>)>> {
    with_ws(state, window, |ws| Ok(ws.catalog())).await
}

/// Run an expression typed in the expression editor. The tab's name refers
/// to its current result, so expressions build on earlier steps.
#[tauri::command(rename_all = "snake_case")]
pub async fn run_expression(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    text: String,
) -> ApiResult<TabSummary> {
    let i18n = lock(&state.config).i18n.clone();
    with_ws(state, window, move |ws| {
        let name = ws.tab_name(id)?.to_string();
        let catalog = catalog_of(ws);
        check_target_table(&text, &catalog, &name, &i18n)?;
        let t = parvu_dsl::translate(&text, &catalog, &FunctionRegistry::default(), &name)?;
        let label = match &t.kind {
            TranslationKind::Assignment { column } => StepLabel::new("step.add_column", &[("column", column)]),
            _ => StepLabel::new("step.expression", &[("expression", &short(&text, 60))]),
        };
        ws.run_relative_sql(id, &t.sql, label)?;
        Ok(ws.summary(id)?)
    })
    .await
}

/// Table-level expressions (assignments, drop_duplicates, drop_null) name
/// their table. Running one that names another tab would replace this tab's
/// data with the other table, so refuse it.
fn check_target_table(text: &str, catalog: &Catalog, active: &str, i18n: &I18n) -> ApiResult<()> {
    use parvu_dsl::Expr;
    let ast = parvu_dsl::parse(text.trim())?;
    let expr = parvu_dsl::resolve(&ast, catalog, &FunctionRegistry::default())?;
    let target = match &expr {
        Expr::Assignment { table, .. } | Expr::DropDuplicates { table, .. } | Expr::DropNull { table, .. } => table,
        _ => return Ok(()),
    };
    if target != active {
        let message = i18n.tr(
            "error.expression_other_table",
            &[("target", target.as_str().into()), ("active", active.into())],
        );
        return Err(ApiError::new("expression", message));
    }
    Ok(())
}

#[derive(Serialize)]
pub struct ExpressionPreview {
    pub sql: String,
    pub kind: Option<TranslationKind>,
}

/// Live SQL preview of an expression, without running it.
#[tauri::command(rename_all = "snake_case")]
pub async fn preview_expression(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    text: String,
) -> ApiResult<ExpressionPreview> {
    with_ws(state, window, move |ws| {
        let name = ws.tab_name(id)?.to_string();
        let t = parvu_dsl::translate(&text, &catalog_of(ws), &FunctionRegistry::default(), &name)?;
        Ok(ExpressionPreview { sql: t.sql, kind: Some(t.kind) })
    })
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn apply_op(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    op: Operation,
) -> ApiResult<TabSummary> {
    with_ws(state, window, move |ws| {
        ws.apply(id, &op)?;
        Ok(ws.summary(id)?)
    })
    .await
}

/// Math operation dialog: add column `name` computed by expression `text`.
#[tauri::command(rename_all = "snake_case")]
pub async fn add_computed_column(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    name: String,
    text: String,
) -> ApiResult<TabSummary> {
    with_ws(state, window, move |ws| {
        let sql = parvu_dsl::preview(&text, &catalog_of(ws), &FunctionRegistry::default())?;
        ws.apply(id, &Operation::AddColumn { name, expression: sql })?;
        Ok(ws.summary(id)?)
    })
    .await
}

#[derive(serde::Deserialize)]
pub struct ReplaceRequest {
    pub column: String,
    pub pattern: String,
    pub replacement: String,
    pub output_column: String,
    pub case_sensitive: bool,
    pub regex: bool,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn replace_values(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    request: ReplaceRequest,
) -> ApiResult<TabSummary> {
    with_ws(state, window, move |ws| {
        let table = ws.tab_name(id)?.to_string();
        let r = request;
        let expression =
            parvu_dsl::compile_replace(&table, &r.column, &r.pattern, &r.replacement, r.case_sensitive, r.regex);
        ws.apply(
            id,
            &Operation::Replace { column: r.column, pattern: r.pattern, output_column: r.output_column, expression },
        )?;
        Ok(ws.summary(id)?)
    })
    .await
}

#[derive(Serialize)]
pub struct Undone {
    pub label: StepLabel,
    pub summary: TabSummary,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn undo(state: State<'_, AppState>, window: WebviewWindow, id: TabId) -> ApiResult<Undone> {
    with_ws(state, window, move |ws| {
        let label = ws.undo(id)?;
        Ok(Undone { label, summary: ws.summary(id)? })
    })
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn reset_tab(state: State<'_, AppState>, window: WebviewWindow, id: TabId) -> ApiResult<TabSummary> {
    with_ws(state, window, move |ws| {
        ws.reset(id)?;
        Ok(ws.summary(id)?)
    })
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn edit_cell(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    row: u64,
    column: String,
    value: Cell,
) -> ApiResult<TabSummary> {
    with_ws(state, window, move |ws| {
        ws.edit_cell(id, row, &column, value)?;
        Ok(ws.summary(id)?)
    })
    .await
}

/// Aggregated or sampled data for a chart of the tab's current result.
#[tauri::command(rename_all = "snake_case")]
pub async fn chart_data(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    spec: parvu_core::chart::ChartSpec,
) -> ApiResult<parvu_core::chart::ChartData> {
    with_ws(state, window, move |ws| Ok(ws.chart(id, &spec)?)).await
}

/// Numeric minimum and maximum of a column, for colour scales.
#[tauri::command(rename_all = "snake_case")]
pub async fn column_range(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    column: String,
) -> ApiResult<Option<(f64, f64)>> {
    with_ws(state, window, move |ws| Ok(ws.numeric_range(id, &column)?)).await
}

/// One cell of a batch edit: absolute row, column and new value.
#[derive(Debug, serde::Deserialize)]
pub struct CellValue {
    pub row: u64,
    pub column: String,
    pub value: Cell,
}

/// Edit several cells as one undoable step (fill down).
#[tauri::command(rename_all = "snake_case")]
pub async fn edit_cells(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    cells: Vec<CellValue>,
) -> ApiResult<TabSummary> {
    with_ws(state, window, move |ws| {
        let edits: Vec<(u64, String, Cell)> = cells.into_iter().map(|c| (c.row, c.column, c.value)).collect();
        ws.edit_cells(id, &edits)?;
        Ok(ws.summary(id)?)
    })
    .await
}

/// Save to the tab's own file (`path` = null) or to `path`.
#[tauri::command(rename_all = "snake_case")]
pub async fn save_tab(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    path: Option<PathBuf>,
) -> ApiResult<TabSummary> {
    with_ws(state, window, move |ws| {
        ws.save(id, path.as_deref())?;
        Ok(ws.summary(id)?)
    })
    .await
}

// ---------------------------------------------------------------- inspection

#[tauri::command(rename_all = "snake_case")]
pub async fn column_stats(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    column: String,
) -> ApiResult<ColumnStats> {
    with_ws(state, window, move |ws| Ok(ws.column_stats(id, &column)?)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn unique_values(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    column: String,
) -> ApiResult<Vec<Cell>> {
    with_ws(state, window, move |ws| Ok(ws.unique_values(id, &column)?)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn column_tuple(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    column: String,
    mode: SampleMode,
    n: u64,
) -> ApiResult<String> {
    with_ws(state, window, move |ws| Ok(ws.column_tuple(id, &column, mode, n.max(1))?)).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn find(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    options: SearchOptions,
) -> ApiResult<Option<SearchHit>> {
    with_ws(state, window, move |ws| Ok(ws.find(id, &options)?)).await
}

// ---------------------------------------------------------------- export (background, cancellable)

#[derive(Clone, Serialize)]
pub struct ExportProgress {
    pub job: u64,
    /// 0-100, or null when DuckDB cannot estimate.
    pub percent: Option<f64>,
}

#[derive(Clone, Serialize)]
pub struct ExportFinished {
    pub job: u64,
    pub ok: bool,
    pub cancelled: bool,
    pub rows: u64,
    pub path: PathBuf,
    pub error: Option<String>,
}

/// Start exporting a tab. Progress arrives as `export-progress` events and
/// the result as one `export-finished` event, both on the calling window.
#[tauri::command(rename_all = "snake_case")]
pub fn start_export(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    options: ExportOptions,
) -> ApiResult<u64> {
    let (conn, query) = {
        let ws = ws_of(&state, &window)?;
        let ws = lock(&ws);
        (ws.clone_connection()?, ws.current_query(id)?)
    };
    conn.set_query_progress_tracking(true).map_err(|e| ApiError::new("query", e.to_string()))?;
    let progress = conn.progress_handle();
    let interrupt = conn.interrupt_handle();
    let job = state.next_job_id();
    let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    lock(&state.exports).insert(job, ExportJob { interrupt, cancelled: cancelled.clone() });

    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let done = done.clone();
        let window = window.clone();
        std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                let p = progress.query_progress();
                let _ = window.emit("export-progress", ExportProgress { job, percent: p.percentage() });
                std::thread::sleep(Duration::from_millis(250));
            }
        });
    }
    let app = window.app_handle().clone();
    std::thread::spawn(move || {
        let result = parvu_core::export::export(&conn, &query, &options);
        done.store(true, Ordering::Relaxed);
        let was_cancelled = cancelled.load(Ordering::Relaxed);
        let finished = match result {
            Ok(rows) => ExportFinished { job, ok: true, cancelled: false, rows, path: options.path.clone(), error: None },
            Err(e) => ExportFinished {
                job,
                ok: false,
                cancelled: was_cancelled,
                rows: 0,
                path: options.path.clone(),
                error: (!was_cancelled).then(|| e.to_string()),
            },
        };
        if let Some(st) = app.try_state::<AppState>() {
            lock(&st.exports).remove(&job);
        }
        let _ = window.emit("export-finished", finished);
    });
    Ok(job)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cancel_export(state: State<'_, AppState>, job: u64) {
    if let Some(j) = lock(&state.exports).get(&job) {
        j.cancelled.store(true, Ordering::Relaxed);
        j.interrupt.interrupt();
    }
}
