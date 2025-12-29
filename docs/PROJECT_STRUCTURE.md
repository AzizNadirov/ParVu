# ParVu Project Structure

## Overview
ParVu is a modern PyQt6 application for viewing and querying large Parquet, CSV, and JSON files using DuckDB.

## Directory Structure

```
ParVu/
├── src/                          # Source code
│   ├── app.py                    # Application entry point
│   ├── engine.py                 # DuckDB query engine with pagination
│   ├── sql_editor.py             # SQL editor with auto-completion
│   ├── table_view.py             # Enhanced table widget
│   ├── main_window.py            # Main application window
│   ├── schemas.py                # Settings and data models
│   ├── __init__.py               # Package marker
│   ├── history/                  # Recent files tracking
│   │   └── recents.json
│   ├── settings/                 # Default settings
│   │   ├── default_settings.json
│   │   └── settings.json
│   └── static/                   # Static assets
│       ├── logo.jpg
│       ├── help.md
│       └── loading-thinking.gif
├── .venv/                        # Virtual environment (uv managed)
├── pyproject.toml                # Project dependencies
├── uv.lock                       # Locked dependencies
├── README.md                     # User documentation
├── MIGRATION.md                  # Migration guide from v1
├── .gitignore                    # Git ignore rules
└── .python-version               # Python version (3.13)
```

## Module Responsibilities

### Core Application (`src/`)

#### `app.py`
- Application entry point
- Initializes PyQt6 application
- Sets up logging
- Handles command-line arguments
- Launches main window

**Key Functions:**
- `main()` - Entry point

---

#### `engine.py`
- DuckDB query engine
- Pagination logic
- File loading (Parquet/CSV/JSON)
- Query execution and validation
- Data export functionality

**Key Classes:**
- `QueryEngine` - Main engine class

**Key Methods:**
- `execute_query(query)` - Execute SQL and update state
- `get_page(page_num)` - Fetch specific page
- `get_unique_values(column)` - Get unique values with warnings
- `sort_by_column(column, ascending)` - Sort results
- `export_results(output_path)` - Export to file

---

#### `sql_editor.py`
- SQL editor widget
- Syntax highlighting
- Auto-completion for keywords and columns

**Key Classes:**
- `SQLSyntaxHighlighter` - Syntax highlighting
- `SQLEditor` - Main editor widget

**Key Methods:**
- `update_completions(column_names)` - Update auto-complete list
- `get_query()` - Get SQL text
- `set_query(query)` - Set SQL text

---

#### `table_view.py`
- Custom table widget
- Cell editing on double-click
- Column context menu
- Unique values dialog

**Key Classes:**
- `DataTableView` - Enhanced table widget
- `UniqueValuesDialog` - Unique values filter dialog

**Key Signals:**
- `sort_requested(column, ascending)` - Sort signal
- `unique_values_requested(column)` - Unique values signal
- `filter_requested(column, value)` - Filter signal

**Key Methods:**
- `load_data(df)` - Load DataFrame into table
- `show_context_menu(pos)` - Show column menu
- `copy_column_as_tuple(column)` - Copy as Python tuple

---

#### `main_window.py`
- Main application window
- Coordinates all components
- Handles user interactions
- Menu bar and toolbar
- Status updates

**Key Classes:**
- `MainWindow` - Main window class
- `QueryWorker` - Background query thread

**Key Methods:**
- `load_file(file_path)` - Load data file
- `execute_query()` - Execute SQL query
- `on_sort_requested(column, ascending)` - Handle sort
- `on_unique_values_requested(column)` - Handle unique values
- `export_results()` - Export data

---

#### `schemas.py`
- Pydantic models for settings
- Settings persistence
- Recent files tracking

**Key Classes:**
- `Settings` - Application settings model
- `Recents` - Recent files model

**Configuration Location:**
- User settings: `~/.ParVu/settings/settings.json`
- Recent files: `~/.ParVu/history/recents.json`
- Log files: `~/.ParVu/logs/parvu_*.log`

---

## Data Flow

```
User Action
    ↓
MainWindow (UI Event)
    ↓
QueryEngine (Execute Query)
    ↓
DuckDB (Process)
    ↓
Paginated Results
    ↓
DataTableView (Display)
    ↓
User Sees Results
```

## Key Design Patterns

### 1. **Separation of Concerns**
- UI logic in `main_window.py`
- Data logic in `engine.py`
- Presentation in `table_view.py` and `sql_editor.py`

### 2. **Signal-Slot Pattern**
- PyQt6 signals for component communication
- Loose coupling between widgets

### 3. **Background Processing**
- `QueryWorker` thread for non-blocking queries
- Prevents UI freezing on large operations

### 4. **Lazy Loading**
- Only current page loaded in memory
- Efficient for files with millions of rows

## Dependencies

### Core
- **PyQt6** (6.6.0+) - GUI framework
- **DuckDB** (1.4.3+) - SQL engine
- **Pandas** (2.3.3+) - Data structures

### Utilities
- **Pydantic** (2.12.5+) - Settings validation
- **Loguru** (0.7.0+) - Logging
- **PyArrow** (15.0.0+) - Parquet support

## Configuration

### Settings (`~/.ParVu/settings/settings.json`)

```json
{
  "default_data_var_name": "data",
  "default_limit": 100,
  "default_sql_font_size": 10,
  "default_result_font_size": 8,
  "default_sql_query": "SELECT * FROM $(default_data_var_name) LIMIT 100",
  "default_sql_font": "Courier",
  "sql_keywords": ["SELECT", "FROM", "WHERE", ...],
  "result_pagination_rows_per_page": "100",
  "save_file_history": "true",
  "max_rows": "10000",
  "colour_browseButton": "#E8F5E9",
  "colour_sqlEdit": "#FFF9C4",
  "colour_executeButton": "#E1F5FE",
  "colour_resultTable": "#FFFFFF",
  "colour_tableInfoButton": "#F3E5F5"
}
```

### Variable Substitution
Settings support variable substitution using `$(variable_name)` syntax.

Example: `"SELECT * FROM $(default_data_var_name)"` → `"SELECT * FROM data"`

## Running the Application

### Development
```bash
uv run python src/app.py
```

### With File
```bash
uv run python src/app.py data.parquet
```

### Production
```bash
# Install dependencies
uv sync

# Run
uv run python src/app.py
```

## Testing

### Manual Testing Checklist
- [ ] Load Parquet file
- [ ] Load CSV file
- [ ] Load JSON file
- [ ] Execute SQL query
- [ ] Navigate pages (prev/next)
- [ ] Sort column (asc/desc)
- [ ] Show unique values
- [ ] Filter by value
- [ ] Edit cell (double-click)
- [ ] Copy column as tuple
- [ ] Export to CSV
- [ ] Export to Parquet
- [ ] Export to JSON
- [ ] Recent files menu
- [ ] Settings persistence

## Future Enhancements

Potential features for future versions:
- [ ] Multi-file joins
- [ ] Query history
- [ ] Saved queries/bookmarks
- [ ] Data visualization
- [ ] Excel export with formatting
- [ ] Custom themes
- [ ] Plugin system
- [ ] Command palette
- [ ] Diff viewer for comparing files

## Troubleshooting

### Import Errors
```bash
# Reinstall dependencies
uv sync
```

### Settings Issues
```bash
# Reset settings
rm -rf ~/.ParVu/settings/
# Restart app (auto-creates defaults)
```

### Performance Issues
- Increase `result_pagination_rows_per_page` for faster navigation
- Use more specific SQL queries
- Avoid unique values on large columns

## Contributing

When adding features:
1. Keep modules focused (single responsibility)
2. Use type hints
3. Add logging for debugging
4. Update this documentation
5. Test with large files (>1M rows)

## License

See repository for license information.
