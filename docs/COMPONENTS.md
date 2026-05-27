# ParVu Component Catalog

## Core Layer (`parvu.core`)

| Component | Responsibility | Key Classes |
|-----------|---------------|-------------|
| `interfaces.py` | Protocols/ABCs for DI | `IQueryEngine`, `IFileAdapter`, `ISettings`, `ITranslator` |
| `models.py` | Domain data models | `Page`, `ColumnInfo`, `FileInfo`, `QueryResult`, `SortSpec` |
| `query_engine.py` | DuckDB wrapper | `QueryEngine` - lazy loading, pagination, undo history |
| `file_adapters.py` | Format detection | `ParquetAdapter`, `CsvAdapter`, `JsonAdapter`, `FileAdapterRegistry` |
| `pagination.py` | Page calculations | `Paginator` - clamp, offset, recalculate |
| `exceptions.py` | Domain errors | `ParVuError`, `QueryError`, `FileFormatError` |

### DSL Subsystem (`parvu.core.dsl`)

| Component | Responsibility | Key Classes |
|-----------|---------------|-------------|
| `ir.py` | Intermediate representation | `Expr`, `ColumnRef`, `Literal`, `Call`, `BinaryOp`, `Assignment`, `DropDuplicates` |
| `parser.py` | Lark grammar parser | `Parser` - parses expressions into AST |
| `resolver.py` | AST → IR transformation | `Resolver` - Lark Tree → typed IR nodes |
| `compiler.py` | IR → SQL compilation | `Compiler` - generates DuckDB SQL via sqlglot |
| `registry.py` | Function definitions | `FunctionRegistry`, `FunctionDef`, `ParamDef` |

## Config Layer (`parvu.config`)

| Component | Responsibility | Key Classes |
|-----------|---------------|-------------|
| `settings.py` | App settings | `Settings` (Pydantic), `SettingsManager` |
| `recents.py` | Recent files | `RecentsManager` |
| `app_state.py` | Runtime state | `AppState` |

## Infrastructure Layer (`parvu.infrastructure`)

| Component | Responsibility | Key Classes |
|-----------|---------------|-------------|
| `i18n/` | Translations | `Locale`, `I18n`, `Translator`, `create_translator()` |
| `themes/` | Theming | `Theme`, `ColorScheme`, `LayoutConfig`, `ThemeManager` |
| `logging_config.py` | Loguru setup | `setup_logging()` |
| `paths.py` | Path utilities | `get_user_data_dir()`, `resolve_resource_path()` |

## Service Layer (`parvu.services`)

| Component | Responsibility | Key Classes |
|-----------|---------------|-------------|
| `container.py` | DI wiring | `ServiceContainer` |
| `window_service.py` | Window mgmt | `WindowService` |
| `query_service.py` | Query ops | `QueryService` - execute, paginate, export |
| `file_service.py` | File ops | `FileService` - validate, recents |
| `session_service.py` | Lifecycle | `SessionService` - crash handler, shutdown |

## Presentation Layer (`parvu.presentation`)

| Component | Responsibility | Key Classes |
|-----------|---------------|-------------|
| `main_window.py` | Main window | `MainWindow` - orchestrates all widgets |
| `themeable.py` | Theme mixin | `ThemeableMixin`, `IThemeable` |
| `workers.py` | Background threads | `QueryWorker`, `ExportWorker`, `UniqueValuesWorker` |
| `widgets/sql_editor.py` | SQL input | `SQLEditor`, `SQLSyntaxHighlighter` |
| `widgets/data_table.py` | Data grid | `DataTableView` |
| `widgets/pagination_bar.py` | Page nav | `PaginationBar` |
| `widgets/file_toolbar.py` | File input | `FileToolbar` |
| `widgets/query_toolbar.py` | Query buttons | `QueryToolbar` |
| `widgets/applied_steps.py` | Steps & undo | `AppliedStepsPanel` - collapsible, undo button |
| `widgets/query_editor.py` | Dual SQL/expr editor | `QueryEditor` - SQL mode + expression mode |
| `dialogs/settings_dialog.py` | Settings UI | `SettingsDialog` |
| `dialogs/theme_selector.py` | Theme picker | `ThemeSelectorDialog` |
| `dialogs/crash_reporter.py` | Crash UI | `CrashReportDialog` |
| `dialogs/about_dialog.py` | Help/About | `AboutDialog` |
| `dialogs/table_info_dialog.py` | Metadata | `TableInfoDialog` |
| `dialogs/unique_values_dialog.py` | Filter | `UniqueValuesDialog` |
| `dialogs/language_selector.py` | Language | `LanguageSelector` |
| `dialogs/drop_duplicates_dialog.py` | Deduplication | `DropDuplicatesDialog` - column checklist + keep strategy |

## Plugin Layer (`parvu.plugins`)

| Component | Responsibility | Key Classes |
|-----------|---------------|-------------|
| `base.py` | Plugin ABC | `Plugin`, `PluginContext` |
| `hooks.py` | Hook system | `HookType`, `HookRegistry` |
| `registry.py` | Plugin mgmt | `PluginRegistry` |
