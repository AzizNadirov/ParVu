# ParVu Project Structure

## Overview
ParVu is a modern PyQt6 application for viewing and querying large Parquet, CSV, and JSON files using DuckDB. The codebase follows a layered architecture under `src/parvu/`.

## Directory Structure

```
ParVu/
├── src/                          # Source code
│   ├── app.py                    # Application entry point (legacy shim)
│   ├── parvu/                    # Main package
│   │   ├── core/                 # Domain layer
│   │   │   ├── dsl/              # Expression language
│   │   │   │   ├── parser.py     # Lark grammar parser
│   │   │   │   ├── resolver.py   # AST → IR transformer
│   │   │   │   ├── compiler.py   # IR → DuckDB SQL
│   │   │   │   ├── registry.py   # Function registry
│   │   │   │   ├── ir.py         # IR node definitions
│   │   │   │   ├── types.py      # Type system
│   │   │   │   └── catalog.py    # Schema catalog for resolver
│   │   │   ├── query_engine.py   # DuckDB engine + pagination + undo
│   │   │   ├── file_adapters.py  # Parquet/CSV/JSON detection
│   │   │   ├── pagination.py     # Page calculations
│   │   │   ├── models.py         # Domain models
│   │   │   ├── edit_queue.py     # Cell edit tracking
│   │   │   ├── interfaces.py     # Protocols/ABCs
│   │   │   └── exceptions.py     # Domain errors
│   │   ├── config/               # Configuration layer
│   │   │   ├── settings.py       # Pydantic settings
│   │   │   ├── recents.py        # Recent files
│   │   │   └── app_state.py      # Runtime state
│   │   ├── infrastructure/       # Infrastructure layer
│   │   │   ├── i18n/             # Translations (en, ru, az)
│   │   │   ├── themes/           # Theme system
│   │   │   ├── logging_config.py # Loguru setup
│   │   │   └── paths.py          # Path utilities
│   │   ├── services/             # Service layer (DI orchestration)
│   │   │   ├── container.py      # DI container
│   │   │   ├── query_service.py  # Query operations
│   │   │   ├── file_service.py   # File operations
│   │   │   ├── window_service.py # Window management
│   │   │   └── session_service.py # Lifecycle / crash handling
│   │   ├── presentation/         # UI layer (PyQt6)
│   │   │   ├── main_window.py    # Main window + menu + undo
│   │   │   ├── themeable.py      # Theme mixin
│   │   │   ├── workers.py        # Background threads
│   │   │   ├── models/           # UI models
│   │   │   │   └── table_tab.py  # Per-tab state (engine, steps, edits)
│   │   │   ├── widgets/          # Reusable widgets
│   │   │   │   ├── data_table.py # Data grid with editing
│   │   │   │   ├── query_editor.py # Dual SQL/expression editor
│   │   │   │   ├── sql_editor.py # SQL input + highlighter
│   │   │   │   ├── expression_editor.py # Expression input
│   │   │   │   ├── applied_steps.py # Collapsible steps panel
│   │   │   │   ├── pagination_bar.py # Page navigation
│   │   │   │   ├── file_toolbar.py # File input toolbar
│   │   │   │   ├── query_toolbar.py # Query buttons
│   │   │   │   ├── ops_pan.py    # Operations toolbar
│   │   │   │   └── tab_bar.py    # Tab management
│   │   │   └── dialogs/          # Dialog windows
│   │   │       ├── settings_dialog.py
│   │   │       ├── drop_duplicates_dialog.py
│   │   │       ├── unique_values_dialog.py
│   │   │       ├── theme_selector.py
│   │   │       ├── language_selector.py
│   │   │       ├── about_dialog.py
│   │   │       ├── crash_reporter.py
│   │   │       ├── table_info_dialog.py
│   │   │       ├── join_dialog.py
│   │   │       └── append_dialog.py
│   │   ├── plugins/              # Plugin system
│   │   │   ├── base.py           # Plugin ABC
│   │   │   ├── hooks.py          # Hook definitions
│   │   │   └── registry.py       # Plugin registry
│   │   ├── utils/                # Utilities
│   │   │   └── typing.py
│   │   └── __main__.py           # `python -m parvu` entry
│   ├── static/                   # Static assets
│   └── ... (legacy root files)   # Old flat modules (phasing out)
├── tests/                        # Test suite
│   ├── unit/                     # Unit tests
│   ├── integration/              # Integration tests
│   ├── ui/                       # UI tests (pytest-qt)
│   └── e2e/                      # End-to-end tests
├── docs/                         # Documentation
├── build/                        # Build artifacts
├── dist/                         # Distribution packages
├── pyproject.toml                # Project config
├── uv.lock                       # Locked dependencies
└── README.md                     # User documentation
```

## Module Responsibilities

### Core Application (`src/parvu/core/`)

#### `query_engine.py`
- DuckDB query engine
- Pagination logic
- File loading (Parquet/CSV/JSON)
- Query execution and validation
- **Undo history stack** (`_history`) for reverting transforms
- Data export functionality

**Key Classes:**
- `QueryEngine` - Main engine class

**Key Methods:**
- `execute_query(query)` - Execute SQL and update state
- `apply_transform(query)` - Apply transform with history push
- `undo()` - Revert to previous query
- `get_page(page_num)` - Fetch specific page
- `get_unique_values(column)` - Get unique values with warnings

#### `dsl/` — Expression Language
- **Parser** (`parser.py`) — Lark grammar for expressions
- **Resolver** (`resolver.py`) — AST → typed IR (`Assignment`, `DropDuplicates`, `Call`, etc.)
- **Compiler** (`compiler.py`) — IR → DuckDB SQL via sqlglot
- **Registry** (`registry.py`) — Built-in function definitions for autocomplete
- **Catalog** (`catalog.py`) — Schema provider for resolver

Example:
```python
data[total] = price * quantity
  → Assignment(...)
  → SELECT *, price * quantity AS "total" FROM data
```

#### `edit_queue.py`
- Tracks pending cell edits per tab
- Edits are applied as a final `UPDATE`-style step

---

### Presentation (`src/parvu/presentation/`)

#### `main_window.py`
- Main application window
- Menu bar: **File → Operations → Help**
- Operations menu: Math, Join, Append, Drop Duplicates
- **Undo dispatch** (`_undo_last_step()`) — routes to engine or edit queue
- Collapsible applied steps panel

#### `models/table_tab.py`
- Per-tab state container:
  - `engine: QueryEngine`
  - `applied_steps: list[str]`
  - `_undo_stack: list[dict]` (parallel to steps)
  - `edit_queue: EditQueue`

#### `widgets/query_editor.py`
- Dual-mode editor: SQL mode ↔ Expression mode
- Expression mode compiles via DSL pipeline
- Auto-completion for functions, columns, tables

#### `widgets/applied_steps.py`
- Collapsible panel listing applied transforms
- Undo button (↩) enabled when steps exist
- Toggle header shows step count

---

### Service Layer (`src/parvu/services/`)

| Component | Responsibility |
|-----------|---------------|
| `container.py` | DI wiring (`ServiceContainer`) |
| `window_service.py` | Window management |
| `query_service.py` | Query execution, pagination, export |
| `file_service.py` | File validation, recents |
| `session_service.py` | Crash handler, shutdown |

---

## Data Flow

```
User Action
    ↓
MainWindow (UI Event)
    ↓
Service Layer (QueryService / FileService)
    ↓
Core Layer
    ├─ SQL path: QueryEngine → DuckDB
    └─ Expression path: Parser → Resolver → Compiler → QueryEngine
    ↓
Paginated Results (pd.DataFrame)
    ↓
DataTableView (Display)
```

## Key Design Patterns

### 1. **Layered Architecture**
- Presentation → Services → Core
- DSL is a sub-layer within Core

### 2. **Dependency Injection**
- `ServiceContainer` wires all dependencies
- No global imports

### 3. **Signal-Slot Pattern**
- PyQt6 signals for loose coupling between widgets
- `AppliedStepsPanel.undo_requested → MainWindow._undo_last_step`

### 4. **Undo Stack**
- `QueryEngine._history` for SQL transforms
- `TableTab._undo_stack` for cell edits
- Unified UX via single Undo button

### 5. **Lazy Loading**
- Only current page loaded in memory
- Efficient for files with millions of rows

## Dependencies

### Core
- **PyQt6** (6.10.0+) - GUI framework
- **DuckDB** (1.2.0+) - SQL engine
- **Pandas** (2.2.0+) - Data structures
- **sqlglot** - SQL generation
- **lark** - Grammar parsing

### Utilities
- **Pydantic** (2.12.5+) - Settings validation
- **Loguru** (0.7.0+) - Logging
- **PyArrow** (15.0.0+) - Parquet support

## Configuration

Settings stored in `~/.ParVu/settings/settings.json`:

```json
{
  "default_data_var_name": "data",
  "result_pagination_rows_per_page": "100",
  "current_theme": "ParVu Light",
  "current_language": "en"
}
```

## Testing

### Manual Testing Checklist
- [ ] Load Parquet / CSV / JSON file
- [ ] Execute SQL query
- [ ] Switch to expression mode and run `data[col] = expr`
- [ ] Use Operations → Drop Duplicates
- [ ] Navigate pages
- [ ] Sort column
- [ ] Show unique values
- [ ] Edit cell (double-click)
- [ ] Undo last step
- [ ] Copy column as tuple
- [ ] Export to CSV / Parquet / JSON
- [ ] Recent files menu
- [ ] Settings persistence

## Future Enhancements

Potential features for future versions:
- [ ] Saved queries / bookmarks
- [ ] Data visualization
- [ ] Excel export with formatting
- [ ] Plugin marketplace
- [ ] Command palette
- [ ] Diff viewer for comparing files

## Contributing

When adding features:
1. Keep modules focused (single responsibility)
2. Use type hints
3. Add logging for debugging
4. Update this documentation
5. Test with large files (>1M rows)

## License

See repository for license information.
