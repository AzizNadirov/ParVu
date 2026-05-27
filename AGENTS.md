# ParVu — Agent Guide

This file helps AI agents navigate the ParVu codebase efficiently.

## Project at a Glance

**ParVu** is a PyQt6 desktop app for viewing/querying Parquet/CSV/JSON via DuckDB.
- **Language**: Python 3.13
- **GUI**: PyQt6 6.10+
- **DB**: DuckDB (in-memory, lazy loading)
- **Build**: `uv` (not pip)
- **Entry**: `src/parvu/__main__.py` or `uv run python -m parvu`

## Architecture

Layers (dependencies flow **inward** only):

```
Presentation  →  Services  →  Infrastructure/Config  →  Core
   (PyQt6)         (DI)         (themes, i18n)       (DuckDB)
```

- **Core** (`parvu.core.*`) — zero external deps except pandas/duckdb/sqlglot/lark
- **DSL** (`parvu.core.dsl.*`) — expression language: Parser → Resolver → Compiler
- **Services** (`parvu.services.*`) — DI container wires everything
- **Presentation** (`parvu.presentation.*`) — all Qt code lives here

## Entry Points

| Path | Purpose |
|------|---------|
| `src/parvu/__main__.py` | `python -m parvu` entry. Creates `ServiceContainer`, `QApplication`, `WindowService` |
| `src/app.py` | Legacy shim. Prefer `__main__.py` |

## Dependency Injection

All components receive deps via `ServiceContainer` (`src/parvu/services/container.py`):

```python
container = ServiceContainer()
container.settings          # Pydantic Settings
container.theme_manager     # ThemeManager
container.translator        # _Translator (i18n)
container.file_service      # FileService
container.query_service     # QueryService
container.create_query_engine(path)  # factory for QueryEngine
```

**Never** import global singletons. Always accept `container` or specific deps as arguments.

## Key Patterns

### 1. Protocols over Inheritance
Interfaces are `typing.Protocol` in `parvu.core.interfaces.py`:
- `IQueryEngine`, `IFileAdapter`, `ISettings`, `ITranslator`, `IThemeManager`

### 2. Signals & Slots
PyQt6 signals connect widgets loosely:
- `AppliedStepsPanel.undo_requested → MainWindow._undo_last_step`
- `DataTableView.cell_edited → MainWindow._on_cell_edited`
- `QueryEditor.mode_changed → MainWindow._on_editor_mode_changed`

### 3. Per-Tab State
`TableTab` (`presentation/models/table_tab.py`) is a dataclass holding:
- `engine: QueryEngine`
- `applied_steps: list[str]`
- `_undo_stack: list[dict]` (parallel to steps)
- `edit_queue: EditQueue`

Every tab has its own engine, steps, and undo stack.

### 4. Query Engine Undo
`QueryEngine._history: list[str]` saves previous queries. `apply_transform()` pushes before overwriting. `undo()` pops and restores.

### 5. DSL Compilation Chain
```
Expression text
  → Parser (Lark grammar)
  → Resolver (Tree → IR: Assignment, DropDuplicates, Call, ...)
  → Compiler (IR → sqlglot → DuckDB SQL)
  → QueryEngine.execute_query()
```

Special-cased functions in resolver (`_resolve_drop_duplicates`) happen **before** registry lookup. Registering in `FunctionRegistry` only enables autocomplete.

## File Responsibility Map

### Core Layer

| File | What it does | Key symbols |
|------|-------------|-------------|
| `core/query_engine.py` | DuckDB wrapper, pagination, undo history | `QueryEngine` |
| `core/file_adapters.py` | Detect format, build reader SQL | `FileAdapterRegistry`, `ParquetAdapter`… |
| `core/pagination.py` | Page math | `Paginator` |
| `core/models.py` | Domain dataclasses | `Page`, `ColumnInfo`, `FileInfo` |
| `core/edit_queue.py` | Pending cell edits | `EditQueue` |
| `core/interfaces.py` | Protocols/ABCs | `IQueryEngine`, `IFileAdapter`, `ISettings`… |
| `core/exceptions.py` | Domain errors | `ParVuError`, `QueryError` |

### DSL Layer

| File | What it does | Key symbols |
|------|-------------|-------------|
| `core/dsl/parser.py` | Lark grammar | `Parser.parse()` |
| `core/dsl/resolver.py` | AST → typed IR | `Resolver.resolve()`, `_resolve_drop_duplicates()` |
| `core/dsl/compiler.py` | IR → SQL | `Compiler.compile()` |
| `core/dsl/ir.py` | IR nodes | `Expr`, `ColumnRef`, `Literal`, `Call`, `Assignment`, `DropDuplicates` |
| `core/dsl/registry.py` | Function registry | `FunctionRegistry`, `FunctionDef`, `ParamDef` |
| `core/dsl/types.py` | Type system | `LogicalType` |
| `core/dsl/catalog.py` | Schema catalog | `Catalog` |

### Presentation Layer

| File | What it does | Key symbols |
|------|-------------|-------------|
| `presentation/main_window.py` | Main window, menus, undo dispatch | `MainWindow` |
| `presentation/models/table_tab.py` | Per-tab state container | `TableTab`, `slugify_name()` |
| `presentation/themeable.py` | Theme mixin | `ThemeableMixin` |
| `presentation/workers.py` | Background threads | `QueryWorker`, `ExportWorker` |

**Widgets:**

| File | What it does | Key symbols |
|------|-------------|-------------|
| `widgets/query_editor.py` | Dual SQL/Expression editor | `QueryEditor` |
| `widgets/sql_editor.py` | SQL input + highlighter | `SQLEditor` |
| `widgets/expression_editor.py` | Expression input | `ExpressionEditor` |
| `widgets/data_table.py` | Data grid | `DataTableView` |
| `widgets/applied_steps.py` | Collapsible steps panel | `AppliedStepsPanel` |
| `widgets/pagination_bar.py` | Page nav | `PaginationBar` |
| `widgets/file_toolbar.py` | File chooser | `FileToolbar` |
| `widgets/query_toolbar.py` | Exec/clear buttons | `QueryToolbar` |
| `widgets/ops_pan.py` | Operations toolbar | `OpsPan` |
| `widgets/tab_bar.py` | Tab management | `TabBar` |

**Dialogs:**

| File | Purpose |
|------|---------|
| `dialogs/drop_duplicates_dialog.py` | Deduplication config |
| `dialogs/settings_dialog.py` | Settings UI |
| `dialogs/theme_selector.py` | Theme picker |
| `dialogs/unique_values_dialog.py` | Unique values filter |
| `dialogs/join_dialog.py` | Join tables |
| `dialogs/append_dialog.py` | Append tables |
| `dialogs/about_dialog.py` | Help/About |
| `dialogs/crash_reporter.py` | Crash report UI |
| `dialogs/table_info_dialog.py` | Table metadata |
| `dialogs/language_selector.py` | Language picker |
| `dialogs/expression_dialog.py` | **Legacy** — being removed |

### Service Layer

| File | What it does | Key symbols |
|------|-------------|-------------|
| `services/container.py` | DI wiring | `ServiceContainer` |
| `services/window_service.py` | Window mgmt | `WindowService` |
| `services/query_service.py` | Query ops | `QueryService` |
| `services/file_service.py` | File ops | `FileService` |
| `services/session_service.py` | Crash handling | `SessionService` |

### Infrastructure

| File | What it does |
|------|-------------|
| `infrastructure/themes/manager.py` | Theme loading/switching |
| `infrastructure/themes/models.py` | `Theme`, `ColorScheme` |
| `infrastructure/themes/stylesheet.py` | QSS generation |
| `infrastructure/i18n/translator.py` | `_Translator` |
| `infrastructure/i18n/base.py` | `I18n` |
| `infrastructure/i18n/locales/en.py` | English strings |
| `infrastructure/i18n/locales/ru.py` | Russian strings |
| `infrastructure/i18n/locales/az.py` | Azerbaijani strings |
| `infrastructure/logging_config.py` | Loguru setup |
| `infrastructure/paths.py` | `resolve_static_path()`, `get_log_dir()` |

### Config

| File | What it does |
|------|-------------|
| `config/settings.py` | Pydantic `Settings`, `SettingsManager` |
| `config/recents.py` | `RecentsManager` |
| `config/app_state.py` | `AppState` |

## Testing

Run tests:
```bash
uv run pytest tests/unit/ -q
```

- **Unit**: `tests/unit/core/dsl/`, `tests/unit/core/test_*.py`, `tests/unit/presentation/`
- **UI tests**: use `pytest-qt` (installed)
- **Fixtures**: `tests/conftest.py` — `temp_settings`, `recents_manager`, `theme_manager`, `paginator`, `adapter_registry`

## Known Pitfalls & Conventions

### UI
1. **Menu icons**: Never call `setIcon()` on `QMenu` objects — it hides text labels on Linux/GTK. Only set icons on `QAction` items inside menus.
2. **Stylesheet timing**: `_apply_theme()` must run **before** `_setup_menu()` in `MainWindow.__init__`, or menu stylesheet rules won't take effect.
3. **File toolbar**: Hidden after a tab is loaded, shown when last tab is closed.
4. **Cell edits**: Double-click edits append to both `applied_steps` and `_undo_stack`. Every step-appending site must push to both.

### DSL
1. **Special-cased functions**: `DROP_DUPLICATES` is handled in `_resolve_func_call()` before registry lookup. Adding it to `FunctionRegistry` only enables autocomplete.
2. **Assignment compilation**: `data[col] = expr` → `SELECT *, expr AS "col" FROM data`
3. **DropDuplicates compilation**: Uses `QUALIFY ROW_NUMBER() OVER (PARTITION BY ...)` pattern.

### Data Flow
1. `_apply_transform()` is the **single point** for SQL transforms — it calls `engine.apply_transform()`, appends step, and refreshes UI.
2. Undo dispatcher (`_undo_last_step()`) routes to:
   - `"cell_edit"` → `EditQueue.remove()`
   - `"sql"` → `QueryEngine.undo()`

### Strings
- All user-facing strings go through `self._t(key, **kwargs)` (translator).
- Add new keys to all three locale files (`en.py`, `ru.py`, `az.py`).

## When You Need To Change…

| Task | Files to touch |
|------|---------------|
| Add a new DSL function | `core/dsl/ir.py` (new node), `core/dsl/resolver.py` (resolve rule), `core/dsl/compiler.py` (compile rule), `core/dsl/registry.py` (for autocomplete) |
| Add a new UI dialog | `presentation/dialogs/`, `presentation/main_window.py` (menu/wiring) |
| Add a new widget | `presentation/widgets/`, `presentation/main_window.py` |
| Change menu structure | `presentation/main_window.py` (`_setup_menu()`) |
| Add new setting | `config/settings.py` (Pydantic model), `dialogs/settings_dialog.py` (UI), locale files (label) |
| Add new theme color | `infrastructure/themes/models.py`, `infrastructure/themes/stylesheet.py` |
| Add new locale | `infrastructure/i18n/locales/`, register in `infrastructure/i18n/base.py` |
| Fix pagination | `core/pagination.py`, `core/query_engine.py` |
| Fix query execution | `core/query_engine.py` |
| Add file format support | `core/file_adapters.py` (new adapter class + register) |

## Useful Commands

```bash
# Run app
uv run python -m parvu

# Run tests
uv run pytest tests/unit/ -q

# Run specific test
uv run pytest tests/unit/core/dsl/test_compiler.py -v

# Build Linux package
./build.sh

# Check types (if mypy configured)
uv run mypy src/parvu
```

## Docs

Human docs live in `docs/`. Key files:
- `docs/DSL.md` — expression language user guide
- `docs/ARCHITECTURE.md` — system architecture
- `docs/COMPONENTS.md` — component catalog
- `docs/PROJECT_STRUCTURE.md` — directory structure
