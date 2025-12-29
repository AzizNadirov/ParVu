# Migration Guide - ParVu v2.0

This guide helps you transition from the old PyQt5 version to the new PyQt6 version.

## What Changed

### Major Changes

1. **PyQt5 → PyQt6**
   - Complete UI framework upgrade
   - Modern Qt6 API with better performance
   - Improved signal/slot system

2. **New Architecture**
   - Separated concerns into modules:
     - `engine.py` - DuckDB query engine with pagination
     - `sql_editor.py` - SQL editor with auto-completion
     - `table_view.py` - Table widget with advanced features
     - `main_window.py` - Main application window
     - `app.py` - Application entry point

3. **Enhanced Features**
   - **Auto-completion**: SQL keywords and column names
   - **Unique values filter**: Excel-like dropdown with search
   - **Better pagination**: More efficient memory usage
   - **Cell editing**: Double-click to edit values
   - **JSON export**: Added JSON to export formats
   - **Large file warnings**: Warns before expensive operations

### Breaking Changes

1. **Entry Point Changed**
   - Old: `python src/main.py`
   - New: `python src/app.py` or `uv run python src/app.py`

2. **Settings Compatibility**
   - Settings format is compatible
   - Same location: `~/.ParVu/settings/settings.json`
   - New fields added (backward compatible)

3. **Dependencies**
   - PyQt5 removed
   - PyQt6 added
   - Loguru added for better logging
   - PyArrow explicitly added

## Installation

### Uninstall Old Version (Optional)

```bash
# If you had PyQt5 installed
pip uninstall pyqt5 pyqt5-qt5
```

### Install New Version

```bash
# Using uv (recommended)
uv sync

# Or using pip
pip install pyqt6 duckdb pandas pydantic loguru pyarrow
```

## Running the Application

### Old Way (Still Works)

```bash
python src/main.py [file_path]
```

The old `main.py` still exists and works with PyQt5 if you need it.

### New Way (Recommended)

```bash
# Using uv
uv run python src/app.py [file_path]

# Or directly
python src/app.py [file_path]
```

## New Features Guide

### 1. SQL Auto-Completion

Type in the SQL editor and auto-completion appears after 2 characters:

```sql
SE[Tab] → SELECT
FR[Tab] → FROM
col[Tab] → column_name
```

Press Tab or Enter to accept suggestions.

### 2. Unique Values Filter

1. Right-click on any column header
2. Select "Show Unique Values..."
3. Search or scroll through values
4. Double-click or click "Filter" to filter by that value

**Note**: Files with >1M rows will show a warning before calculating unique values.

### 3. Cell Editing

1. Double-click any cell
2. Edit the value
3. Press Enter to confirm
4. Changes are view-only (not saved to file)
5. Export results to save edited data

### 4. Column Operations

Right-click column headers for:

- **Copy Column Name** - Copy to clipboard
- **Sort Ascending/Descending** - Sort table
- **Copy Values as Tuple** - Copy entire column as Python tuple
- **Show Unique Values** - Open unique values dialog

### 5. Export Options

File → Export Results supports:

- **CSV** - Universal format
- **Parquet** - Efficient columnar format
- **JSON** - API-friendly format

## Keyboard Shortcuts

| Action | Shortcut |
|--------|----------|
| Accept auto-completion | Tab or Enter |
| Close completer | Escape |
| Navigate table | Arrow keys |
| Edit cell | Double-click |

## Troubleshooting

### Import Errors

If you see `ImportError: cannot import name 'X' from 'PyQt5'`:

```bash
# Reinstall dependencies
uv sync
# or
pip install --force-reinstall pyqt6
```

### Settings Not Loading

Settings are backward compatible. If you have issues:

1. Backup: `~/.ParVu/settings/settings.json`
2. Delete the settings directory
3. Restart the app (auto-creates defaults)
4. Restore your custom settings

### Performance Issues

For very large files (>10M rows):

1. Increase page size in settings: `result_pagination_rows_per_page`
2. Use more specific SQL queries
3. Avoid calculating unique values on large columns

## File Locations

| Item | Location |
|------|----------|
| Settings | `~/.ParVu/settings/settings.json` |
| Recent files | `~/.ParVu/history/recents.json` |
| Log files | `~/.ParVu/logs/parvu_*.log` |

## Rollback to Old Version

If you need to use the old PyQt5 version:

1. Keep both versions installed
2. Run old version: `python src/main.py`
3. Run new version: `python src/app.py`

Both can coexist and share the same settings.

## Getting Help

- GitHub Issues: https://github.com/AzizNadirov/ParVu/issues
- Check logs: `~/.ParVu/logs/`
- Telegram: @aziz_nadirov

## Summary of Benefits

| Feature | Old | New |
|---------|-----|-----|
| UI Framework | PyQt5 | PyQt6 ✨ |
| SQL Auto-complete | No | Yes ✨ |
| Unique Values Filter | No | Yes ✨ |
| Cell Editing | No | Yes ✨ |
| JSON Export | No | Yes ✨ |
| Pagination Performance | Good | Better ✨ |
| Code Organization | Single file | Modular ✨ |
| Logging | Basic | Advanced ✨ |
| Large File Warnings | No | Yes ✨ |

✨ = New or improved feature
