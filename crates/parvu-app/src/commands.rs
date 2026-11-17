//! Tauri commands: the whole interface between the UI and the Rust crates.
//!
//! Commands that touch data run on the blocking thread pool so the UI thread
//! never waits on DuckDB. Each window has its own workspace, found by the
//! calling window's label.

use crate::column_types::ColumnTypes;
use parvu_core::colfn::{ColumnFn, JsonKey};
use parvu_core::formula::Formula;
use parvu_core::types::{CsvOptions, Reading, SourceColumn};
use parvu_core::workspace::CsvPreview;
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
use parvu_core::stats::{ColumnStats, SampleMode, UniqueValues};
use parvu_core::{TabId, TabSummary, Workspace};
use parvu_dsl::{Catalog, FunctionDef, FunctionRegistry, TranslationKind};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::error::{ApiError, ApiResult};
use crate::jobs::run_job;
use crate::state::{AppState, ExportJob, lock};

type Ws = Arc<Mutex<Workspace>>;

pub(crate) fn ws_of(state: &AppState, window: &WebviewWindow) -> ApiResult<Ws> {
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
        let result = f(&mut guard);
        // A CSV value deep in the file didn't parse: the workspace re-reads
        // that column as text, and the UI repeats the call (kind "healed").
        if let Err(e) = &result
            && guard.heal(&e.message)
        {
            return Err(ApiError::new("healed", e.message.clone()));
        }
        result
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

/// Run `f` on every window's workspace, off the main thread: a workspace
/// busy with a long query must not freeze the window.
async fn for_each_workspace(
    state: &AppState,
    f: impl Fn(&mut Workspace) -> ApiResult<()> + Send + 'static,
) -> ApiResult<()> {
    let all: Vec<Ws> = lock(&state.workspaces).values().cloned().collect();
    tauri::async_runtime::spawn_blocking(move || {
        for ws in all {
            f(&mut lock(&ws))?;
        }
        Ok(())
    })
    .await
    .map_err(|e| ApiError::new("app", e.to_string()))?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_settings(
    state: State<'_, AppState>,
    values: serde_json::Map<String, serde_json::Value>,
) -> ApiResult<Settings> {
    let (page_size, memory_mb, detect) = {
        let mut cfg = lock(&state.config);
        let rejected = cfg.settings.update(values)?;
        if !rejected.is_empty() {
            tracing::warn!("rejected settings: {rejected:?}");
        }
        let s = cfg.settings.settings();
        (s.rows_per_page(), s.query_memory_limit_mb.max(0) as u64, crate::state::detect_options(s))
    };
    let env_limit = std::env::var_os("PARVU_MEMORY_LIMIT_MB").is_some();
    for_each_workspace(&state, move |ws| {
        ws.set_page_size(page_size);
        ws.set_detect_options(detect);
        if !env_limit {
            ws.set_memory_limit_mb(memory_mb)?;
        }
        Ok(())
    })
    .await?;
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
pub async fn set_language(app: AppHandle, state: State<'_, AppState>, code: String) -> ApiResult<LanguageChange> {
    let (change, detect) = {
        let mut cfg = lock(&state.config);
        if !cfg.i18n.set_language(&code) {
            return Err(ApiError::new("config", format!("Unknown language: {code}")));
        }
        let mut values = serde_json::Map::new();
        values.insert("current_language".into(), code.clone().into());
        cfg.settings.update(values)?;
        let change = LanguageChange { language: code.clone(), translations: I18n::catalog(&code).clone() };
        // The automatic date order follows the language.
        (change, crate::state::detect_options(cfg.settings.settings()))
    };
    for_each_workspace(&state, move |ws| {
        ws.set_detect_options(detect);
        Ok(())
    })
    .await?;
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
    options: Option<CsvOptions>,
) -> ApiResult<TabSummary> {
    let p = path.clone();
    let summary = with_ws(state.clone(), window, move |ws| {
        // Options given now ("Open with options") are remembered for the
        // file; otherwise the ones remembered before apply.
        let file = ColumnTypes::file();
        let mut store = ColumnTypes::load(&file);
        let csv = match options {
            Some(o) => {
                store.remember_csv(&p, o.clone());
                if let Err(e) = store.save(&file) {
                    tracing::warn!("could not save {}: {e}", file.display());
                }
                o
            }
            None => store.csv_for(&p).unwrap_or_default(),
        };
        let id = ws.open_with(&p, &csv)?;
        // Types the user chose for this file before ("Read as").
        let chosen = ColumnTypes::load(&ColumnTypes::file()).for_file(&p);
        if let Err(e) = ws.apply_source_readings(id, &chosen) {
            tracing::warn!("could not apply the column types chosen for {}: {e}", p.display());
        }
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
    state.jobs.cancel_tab(window.label(), id);
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

/// Add or change (by `name`) a column formula written in the expression
/// language, e.g. `= [qty] * [price]`. Live: the column recomputes when
/// cells change, and cell editing stays allowed.
#[tauri::command(rename_all = "snake_case")]
pub async fn set_formula(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    name: String,
    source: String,
    after: Option<String>,
) -> ApiResult<TabSummary> {
    with_ws(state, window, move |ws| {
        let table = ws.tab_name(id)?.to_string();
        // Formulas see the file's columns and the formulas', not the steps'.
        let mut catalog = catalog_of(ws);
        catalog.register_table(&table, ws.formula_columns(id)?.into_iter().map(|c| (c.name, c.dtype)));
        let sql = parvu_dsl::compile_formula(&source, &catalog, &FunctionRegistry::default(), &table)?;
        let source = source.trim().trim_start_matches('=').trim().to_string();
        ws.set_formula(id, Formula { name, source, sql, after })?;
        Ok(ws.summary(id)?)
    })
    .await
}

/// Remove a formula column (undoable).
#[tauri::command(rename_all = "snake_case")]
pub async fn remove_formula(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    name: String,
) -> ApiResult<TabSummary> {
    with_ws(state, window, move |ws| {
        ws.remove_formula(id, &name)?;
        Ok(ws.summary(id)?)
    })
    .await
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

/// The first rows of a CSV file read with `options`, for "Open with options".
#[tauri::command(rename_all = "snake_case")]
pub async fn preview_csv(
    state: State<'_, AppState>,
    window: WebviewWindow,
    path: PathBuf,
    options: CsvOptions,
) -> ApiResult<CsvPreview> {
    with_ws(state, window, move |ws| Ok(ws.preview_csv(&path, &options, 50)?)).await
}

/// How each column of a tab's CSV file is read (empty for other formats).
#[tauri::command(rename_all = "snake_case")]
pub async fn source_columns(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
) -> ApiResult<Vec<SourceColumn>> {
    with_ws(state, window, move |ws| Ok(ws.source_columns(id)?.to_vec())).await
}

/// Read a column of the tab's CSV file another way ("Read as" in the column
/// menu), and remember the choice for this file.
#[tauri::command(rename_all = "snake_case")]
pub async fn set_source_reading(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    column: String,
    reading: Reading,
) -> ApiResult<TabSummary> {
    with_ws(state, window, move |ws| {
        ws.set_source_reading(id, &column, reading.clone())?;
        let summary = ws.summary(id)?;
        let file = ColumnTypes::file();
        let mut store = ColumnTypes::load(&file);
        store.remember(&summary.path, &column, reading);
        if let Err(e) = store.save(&file) {
            tracing::warn!("could not save {}: {e}", file.display());
        }
        Ok(summary)
    })
    .await
}

#[derive(Serialize)]
pub struct ConversionPreviewRow {
    before: Cell,
    after: Cell,
    failed: bool,
}

/// The Convert dialog's live preview: the first `n` values before and after.
#[tauri::command(rename_all = "snake_case")]
pub async fn preview_conversion(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    column: String,
    to: Reading,
    n: Option<u64>,
) -> ApiResult<Vec<ConversionPreviewRow>> {
    with_ws(state, window, move |ws| {
        let rows = ws.preview_conversion(id, &column, &to, n.unwrap_or(50).clamp(1, 500))?;
        Ok(rows.into_iter().map(|(before, after, failed)| ConversionPreviewRow { before, after, failed }).collect())
    })
    .await
}

/// How ParVu would read a column, for the Convert dialog's first choice.
#[tauri::command(rename_all = "snake_case")]
pub async fn suggest_reading(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    column: String,
) -> ApiResult<Reading> {
    with_ws(state, window, move |ws| Ok(ws.suggest_reading(id, &column)?)).await
}

/// Preview of a column function over a few values from the page:
/// `[before, after]` pairs.
#[tauri::command(rename_all = "snake_case")]
pub async fn preview_column_fn(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    column: String,
    func: ColumnFn,
    values: Vec<Cell>,
) -> ApiResult<Vec<(Cell, Cell)>> {
    let values: Vec<Cell> = values.into_iter().take(20).collect();
    with_ws(state, window, move |ws| Ok(ws.preview_column_fn(id, &column, &func, &values)?)).await
}

/// Rows sampled for the keys of a JSON column, unless `full`.
const JSON_KEY_SAMPLE_ROWS: u64 = 2_000;

/// The keys of a JSON object column and their types: from the first rows,
/// or (`full`) from every row, for "Expand into columns".
#[tauri::command(rename_all = "snake_case")]
pub async fn json_keys(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    column: String,
    full: bool,
    job: u64,
) -> ApiResult<Vec<JsonKey>> {
    let limit = (!full).then_some(JSON_KEY_SAMPLE_ROWS);
    run_job(
        state,
        window,
        job,
        "json_keys",
        Some(id),
        move |ws| ws.json_keys_query(id, &column, limit),
        |ctx, sql| parvu_core::workspace::json_keys_from(ctx.conn, sql),
    )
    .await
}

/// Rows checked when counting errors without `full`.
const ERROR_SAMPLE_ROWS: u64 = 100_000;

#[derive(Serialize)]
pub struct ConversionCount {
    errors: u64,
    checked: u64,
    /// Every row was checked (not just the first ones).
    complete: bool,
}

/// Values of `column` that don't convert as `to`: on the first 100,000 rows,
/// or, with `full`, on every row. Runs as a job: the window stays usable and
/// the count can be cancelled.
#[tauri::command(rename_all = "snake_case")]
pub async fn conversion_error_count(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    column: String,
    to: Reading,
    full: bool,
    job: u64,
) -> ApiResult<ConversionCount> {
    let limit = (!full).then_some(ERROR_SAMPLE_ROWS);
    let (errors, checked) = run_job(
        state,
        window,
        job,
        if full { "count_full" } else { "count" },
        Some(id),
        move |ws| ws.conversion_error_query(id, &column, &to, limit),
        |ctx, sql| parvu_core::workspace::conversion_errors(ctx.conn, sql),
    )
    .await?;
    Ok(ConversionCount { errors, checked, complete: limit.is_none_or(|n| checked < n) })
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
        let summary = ws.summary(id)?;
        // Undoing "Read as" also forgets the choice for this file.
        if label.key == "step.source_type"
            && let Some(column) = label.args.get("column")
        {
            let file = ColumnTypes::file();
            let mut store = ColumnTypes::load(&file);
            store.forget(&summary.path, column);
            if let Err(e) = store.save(&file) {
                tracing::warn!("could not save {}: {e}", file.display());
            }
        }
        Ok(Undone { label, summary })
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
    job: u64,
) -> ApiResult<parvu_core::chart::ChartData> {
    run_job(state, window, job, "chart", Some(id), move |ws| ws.chart_input(id), move |ctx, (source, columns)| {
        parvu_core::chart::chart_data(ctx.conn, source, columns, &spec)
    })
    .await
}

/// Save a chart image the UI rendered (PNG bytes or SVG text) to the path
/// the user chose in the save dialog. Only `.png` and `.svg` files.
#[tauri::command(rename_all = "snake_case")]
pub fn save_chart_image(path: PathBuf, bytes: Vec<u8>) -> ApiResult<()> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if ext != "png" && ext != "svg" {
        return Err(ApiError::new("export", "Charts are saved as .png or .svg files."));
    }
    std::fs::write(&path, bytes).map_err(|e| ApiError::new("export", format!("{}: {e}", path.display())))
}

/// Numeric minimum and maximum of columns, for colour scales: one scan.
#[tauri::command(rename_all = "snake_case")]
pub async fn column_ranges(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    columns: Vec<String>,
    job: u64,
) -> ApiResult<Vec<Option<(f64, f64)>>> {
    run_job(
        state,
        window,
        job,
        "ranges",
        Some(id),
        move |ws| {
            for c in &columns {
                ws.column_type(id, c)?;
            }
            Ok((ws.unordered_query(id)?, columns))
        },
        |ctx, (source, columns)| parvu_core::stats::numeric_ranges(ctx.conn, source, columns),
    )
    .await
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
    job: u64,
) -> ApiResult<ColumnStats> {
    let name = column.clone();
    run_job(state, window, job, "stats", Some(id), move |ws| ws.stats_input(id, &column), move |ctx, (source, dtype, approx)| {
        parvu_core::stats::column_stats(ctx.conn, source, &name, dtype, *approx)
    })
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn unique_values(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    column: String,
    job: u64,
) -> ApiResult<UniqueValues> {
    let name = column.clone();
    run_job(
        state,
        window,
        job,
        "unique",
        Some(id),
        move |ws| {
            ws.column_type(id, &column)?;
            ws.unordered_query(id)
        },
        move |ctx, source| {
            // New values go to the dialog as they are found.
            parvu_core::stats::unique_values(ctx.conn, source, &name, ctx.cancel, &mut |rows, fresh| {
                ctx.progress(Some(rows), Some(serde_json::json!(fresh)))
            })
        },
    )
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn column_tuple(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    column: String,
    mode: SampleMode,
    n: u64,
    job: u64,
) -> ApiResult<String> {
    let name = column.clone();
    run_job(
        state,
        window,
        job,
        "tuple",
        Some(id),
        move |ws| {
            ws.column_type(id, &column)?;
            ws.current_query(id)
        },
        move |ctx, source| {
            let values = parvu_core::stats::column_sample(ctx.conn, source, &name, mode, n.max(1))?;
            Ok(parvu_core::stats::python_tuple(&values))
        },
    )
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn find(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    options: SearchOptions,
    job: u64,
) -> ApiResult<Option<SearchHit>> {
    run_job(state, window, job, "find", Some(id), move |ws| ws.current_query(id), move |ctx, source| {
        parvu_core::search::find_next(ctx.conn, source, &options)
    })
    .await
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
pub async fn start_export(
    state: State<'_, AppState>,
    window: WebviewWindow,
    id: TabId,
    options: ExportOptions,
) -> ApiResult<u64> {
    let ws_arc = ws_of(&state, &window)?;
    let (conn, query) = {
        // Off the main thread: the workspace may be busy.
        let ws_arc = ws_arc.clone();
        tauri::async_runtime::spawn_blocking(move || -> ApiResult<_> {
            let ws = lock(&ws_arc);
            Ok((ws.clone_connection()?, ws.current_query(id)?))
        })
        .await
        .map_err(|e| ApiError::new("app", e.to_string()))??
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
        // A CSV value deep in the file that doesn't parse: re-read that
        // column as text (Workspace::heal) and write the file again.
        let mut result = parvu_core::export::export(&conn, &query, &options);
        let mut tries = 0;
        while let Err(e) = &result {
            if tries >= 8 || cancelled.load(Ordering::Relaxed) || !lock(&ws_arc).heal(&e.to_string()) {
                break;
            }
            tries += 1;
            result = parvu_core::export::export(&conn, &query, &options);
        }
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
