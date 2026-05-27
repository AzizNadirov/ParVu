# ParVu Code Map

> One-line index of every module. Use Ctrl+F / grep to jump.

## Core (`src/parvu/core/`)

```
interfaces.py          Protocols: IQueryEngine, IFileAdapter, ISettings, ITranslator, IThemeManager
models.py              Domain models: Page, ColumnInfo, FileInfo, QueryResult, SortSpec
query_engine.py        DuckDB engine: execute, paginate, undo history, export
file_adapters.py       Format detection: ParquetAdapter, CsvAdapter, JsonAdapter, Registry
pagination.py          Page math: Paginator (clamp, offset, recalculate)
edit_queue.py          Pending cell edits: EditQueue
exceptions.py          Domain errors: ParVuError, QueryError, FileFormatError
```

### DSL (`src/parvu/core/dsl/`)

```
parser.py              Lark grammar → AST
resolver.py            AST → IR (special-cases DROP_DUPLICATES before registry)
compiler.py            IR → sqlglot → DuckDB SQL
ir.py                  IR nodes: Expr, ColumnRef, Literal, Call, BinaryOp, Assignment, DropDuplicates
registry.py            Function registry + autocomplete metadata
 types.py               LogicalType enum
catalog.py             Schema catalog for resolver
```

## Config (`src/parvu/config/`)

```
settings.py            Pydantic Settings + SettingsManager
recents.py             Recent files persistence
app_state.py           Runtime AppState
```

## Infrastructure (`src/parvu/infrastructure/`)

```
i18n/base.py           I18n orchestrator
i18n/translator.py     _Translator class
themes/manager.py      Theme loading/switching
themes/models.py       Theme, ColorScheme, LayoutConfig
themes/stylesheet.py   QSS generation from theme
themes/builtin/*.py    Built-in themes: light, excel, black
logging_config.py      Loguru setup
paths.py               resolve_static_path(), get_log_dir(), get_legacy_app_dir()
```

## Services (`src/parvu/services/`)

```
container.py           ServiceContainer — DI wiring for ALL components
window_service.py      WindowService — create/remove windows
query_service.py       QueryService — execute, paginate, export
file_service.py        FileService — validate, recents
session_service.py     SessionService — crash handler, shutdown
```

## Presentation (`src/parvu/presentation/`)

```
main_window.py         MainWindow — menus, tabs, undo dispatch, theme, ops
themeable.py           ThemeableMixin — apply_theme_to_widget()
workers.py             QueryWorker, ExportWorker, UniqueValuesWorker
```

### Models (`presentation/models/`)

```
table_tab.py           TableTab dataclass: engine, steps, undo_stack, edit_queue
```

### Widgets (`presentation/widgets/`)

```
query_editor.py        QueryEditor — dual SQL/Expression mode toggle
sql_editor.py          SQLEditor — QTextEdit + SQLSyntaxHighlighter
expression_editor.py   ExpressionEditor — QLineEdit with function autocomplete
data_table.py          DataTableView — QTableView + cell editing + context menu
applied_steps.py       AppliedStepsPanel — collapsible list + undo button
pagination_bar.py      PaginationBar — prev/next + page label
file_toolbar.py        FileToolbar — browse button + path label
query_toolbar.py       QueryToolbar — execute/clear/mode toggle
ops_pan.py             OpsPan — operations toolbar buttons
tab_bar.py             TabBar — tab management
```

### Dialogs (`presentation/dialogs/`)

```
drop_duplicates_dialog.py   Column checklist + first/last radio
settings_dialog.py          Settings tabs: General, Theme, Advanced, Warnings
theme_selector.py           Theme list + preview
unique_values_dialog.py     Unique values filter + search
join_dialog.py              Table join config
append_dialog.py            Table append config
about_dialog.py             Help / About
crash_reporter.py           Crash report form
table_info_dialog.py        Column metadata display
language_selector.py        Language picker
expression_dialog.py        LEGACY — being removed
```

## Tests (`tests/`)

```
conftest.py            Fixtures: temp_settings, recents_manager, theme_manager, paginator, adapter_registry
unit/core/dsl/         Parser, resolver, compiler tests
unit/core/             query_engine, pagination, file_adapters, edit_queue
unit/presentation/     applied_steps, query_editor, table_tab
unit/infrastructure/   theme tests
unit/config/           settings tests
```

## Entry Points

```
src/parvu/__main__.py   python -m parvu [file]
src/app.py              Legacy entry shim
```

## Quick Lookup Table

| Looking for… | Go to… |
|-------------|--------|
| Add DSL function | `core/dsl/ir.py` → `resolver.py` → `compiler.py` → `registry.py` |
| Fix SQL generation | `core/dsl/compiler.py` or `core/query_engine.py` |
| Fix autocomplete | `core/dsl/registry.py` or `widgets/expression_editor.py` |
| Add menu item | `presentation/main_window.py` `_setup_menu()` |
| Add setting | `config/settings.py` → `dialogs/settings_dialog.py` → locales |
| Fix cell edit | `widgets/data_table.py` + `models/table_tab.py` + `core/edit_queue.py` |
| Fix undo | `core/query_engine.py` (SQL) + `presentation/main_window.py` `_undo_last_step()` |
| Fix pagination | `core/pagination.py` + `core/query_engine.py` |
| Fix theme | `infrastructure/themes/*.py` + `presentation/themeable.py` |
| Fix i18n string | `infrastructure/i18n/locales/*.py` + caller using `self._t()` |
| Add test | Mirror path under `tests/unit/` |
