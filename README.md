
# ParVu - **Par**quet **Vie**wer

<div align="center">
  <img src="docs/images/parvu-logo.png" alt="ParVu Logo" width="200"/>
</div>


## Overview

A fast desktop application for viewing and querying large Parquet, CSV, JSON and Excel files. Built with [Rust](https://www.rust-lang.org/), [Tauri](https://tauri.app/), [React](https://react.dev/) and [DuckDB](https://duckdb.org/).

Version 1.0 is a rewrite of the earlier Python/PyQt6 app. It reads the same `~/.ParVu` settings, recent files and themes, so upgrading needs no migration. The rewrite is described in [plans/RUST_REWRITE_PLAN.md](plans/RUST_REWRITE_PLAN.md).

## Features

### File Support
- **Parquet** (`.parquet`, `.pq`), **CSV** (`.csv`, `.tsv`), **JSON** (`.json`, `.jsonl`, `.ndjson`) and **Excel** (`.xlsx`, `.xls`)
- **Lazy loading** - Parquet, CSV and JSON are queried in place, never loaded into memory
- **Huge files** (8GB+) with pagination; only the visible page is fetched

### SQL Querying
- Full DuckDB SQL
- Syntax highlighting and auto-completion for keywords, tables and columns
- Errors are reported before anything changes

### Expression Language (DSL)
- **Two editor modes** - switch between SQL and expression mode
- **Column assignment** - `data[new_col] = data[old_col] * 2` adds or overwrites a column
- **Built-in functions** - `ABS`, `UPPER`, `LEN`, `ROUND`, `REPLACE`, `DROP_DUPLICATES`, `DROP_NULL` and more
- **Auto-completion** for tables, columns, functions and methods, with function documentation
- **Live SQL preview** of what the expression compiles to

### Table Operations
- **Pagination** with a configurable page size and a go-to-page box
- **Cell editing** - type into any cell of the unmodified file; invalid values are rejected
- **Applied steps** - every query, transform and cell edit is listed, and undo reverts the last one (`Ctrl+Z`)
- **Save** (`Ctrl+S`) writes edits and transforms back to the file; **Save As** writes Parquet, CSV, JSON or Excel
- **Sort** by clicking a column header (toggles ascending/descending)
- **Drop duplicates**, **drop null values** (or a sentinel such as `-1`), **replace values** (text or regex)
- **Join** and **append** with other open tabs, using each tab's current result
- **Math operations** - add a column computed by an expression
- **Column statistics** - rows, non-null, null %, distinct, min, max, mean, std
- **Unique values filter** - pick values from a searchable list
- **Copy** - selection as TSV (`Ctrl+C`), CSV, Markdown, or with headers; column values as a tuple
- **Drag and drop** files onto the window to open them as tabs
- **Large dataset warning** before expensive operations

### Search
- **Search** (`Ctrl+F`) finds matches across every page, not just the visible one. Search one column or all, with match case, whole cell and regex options. `F3` / `Shift+F3` step through matches, and the table jumps to the right page and cell.

### Export
- CSV, TSV, Parquet, JSON, JSON Lines and Excel
- Pick columns, cast types, and format dates
- CSV delimiter, header and quoting; Parquet compression
- Progress bar and cancel for large exports

### User Interface
- **Themes** - 3 built-in themes (ParVu Light, Excel, ParVu Black), plus custom themes you can import and export
- **Languages** - English, Russian and Azerbaijani, switchable at any time
- **Multiple tabs and windows** (`Ctrl+N` opens a new window)
- **Shortcut cheatsheet** - `Ctrl+/`
- **Exit warning** for unsaved edits and transforms
- **Recent files** and a crash reporter

## Installation

### For End Users

- **Linux (Ubuntu/Debian)**: download the `.deb` package
  ```bash
  sudo dpkg -i parvu_<version>_amd64.deb
  parvu
  ```
- **Windows**: download and run `parvu_<version>_x64-setup.exe`. It adds a Start menu entry and file associations.

See [RELEASES.md](RELEASES.md) for details.

### For Developers

Requirements: [Rust](https://rustup.rs/) and Node.js 20 or newer. On Debian/Ubuntu also:

```bash
sudo apt install build-essential libwebkit2gtk-4.1-dev libsoup-3.0-dev \
     libjavascriptcoregtk-4.1-dev libgtk-3-dev librsvg2-dev
```

Run from source:

```bash
git clone https://github.com/AzizNadirov/ParVu.git
cd ParVu
npm --prefix ui ci
cd crates/parvu-app && npx --prefix ../../ui tauri dev
```

Build the installer (`.deb` on Linux, `setup.exe` on Windows):

```bash
./build.sh           # package "parvu"
./build.sh --beta    # package "parvu-next", installs next to "parvu"
```

Run the tests:

```bash
cargo test -p parvu-core -p parvu-dsl -p parvu-config
npm --prefix ui test
```

The first build compiles DuckDB from C++ source, which takes several minutes. `.cargo/config.toml` limits cargo to 4 parallel jobs so the build fits in 16 GB of RAM.

| Path | Contents |
|------|----------|
| `crates/parvu-core` | DuckDB engine, tabs, cell edits, operations, export, statistics, search |
| `crates/parvu-dsl` | Expression language parser and compiler |
| `crates/parvu-config` | Settings, recent files, translations, themes |
| `crates/parvu-app` | Tauri app: windows and commands |
| `ui/` | React UI |

See [docs/BUILDING.md](docs/BUILDING.md) and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for more.

## How to Use

### Launch

```bash
parvu
parvu path/to/file.parquet another.csv
```

### Basic Workflow

1. **Open a file** - drop it onto the window, use File → Open (`Ctrl+O`), or paste a path
2. **Browse** - use the pagination bar or type a page number
3. **Query** - write SQL in the editor and press **Execute** or `Ctrl+Enter`. The table is named after the file (`sales.parquet` → `sales`)
4. **Expression mode** - switch the editor to Expression and write, for example:
   - `sales[discount] = sales[price] * 0.1` to add a computed column
   - `DROP_DUPLICATES(sales[id], sales[name], 'first')` to deduplicate
5. **Column menu** - right-click a header to sort, rename, change type, duplicate, remove, copy values, see unique values or statistics, replace values or drop nulls
6. **Cell menu** - right-click cells to copy them as TSV, CSV or Markdown, filter by a value, or set it to NULL
7. **Edit** - type into a cell; edits are highlighted and listed as steps
8. **Undo** - `Ctrl+Z` or the Undo button in Applied Steps
9. **Save** - `Ctrl+S` writes edits and transforms to the file; Applied Steps clears afterwards
10. **Export** - File → Export (`Ctrl+E`)
11. **Search** - `Ctrl+F` searches every row on every page
12. **Theme and language** - View → Theme / Language

## Expression Language Examples

```text
-- Add a computed column
sales[total] = sales[price] * sales[quantity]

-- Remove duplicates keeping the first occurrence
DROP_DUPLICATES(sales[id], sales[category], 'first')

-- Drop rows where a column is NULL (or matches a sentinel)
DROP_NULL(sales[email])
DROP_NULL(sales[score], -1)

-- String and math functions
sales[full_name] = CONCAT(UPPER(sales[first_name]), ' ', UPPER(sales[last_name]))
sales[abs_diff] = ABS(sales[price] - sales[avg_price])
```

See [docs/DSL.md](docs/DSL.md) for the full language guide.

## Example SQL Queries

```sql
-- Filter rows
SELECT * FROM sales WHERE age > 25

-- Aggregate
SELECT category, COUNT(*) AS count, AVG(price) AS avg_price
FROM sales
GROUP BY category
ORDER BY count DESC

-- Join two open files
SELECT * FROM sales JOIN customers USING (customer_id)
```

## Configuration

Settings are stored in `~/.ParVu/settings/settings.json` and can be changed in File → Settings. Logs are in `~/.ParVu/logs`.

### Themes

- **ParVu Light** - default light theme
- **Excel** - Excel-inspired green theme
- **ParVu Black** - dark theme

Custom themes live in `~/.ParVu/themes`. See [docs/THEMES.md](docs/THEMES.md).

### Languages

- 🇬🇧 **English** - default
- 🇷🇺 **Russian** (Русский)
- 🇦🇿 **Azerbaijani** (Azərbaycan)

Change the language in View → Language. See [docs/I18N.md](docs/I18N.md) to add a translation.

## FAQ

**Q: What SQL is supported?**
A: All of DuckDB's SQL, including joins, aggregations and window functions. See the [DuckDB documentation](https://duckdb.org/docs/sql/query_syntax/select).

**Q: Does editing change the file?**
A: Only when you save. Edits are kept as pending steps until you press `Ctrl+S` or use Save As.

**Q: How large a file can it handle?**
A: Parquet, CSV and JSON files are queried in place by DuckDB, and only the visible page is loaded, so files of 8GB and more work. Excel files are read into memory when opened.

**Q: What changed from the Python version?**
A: The app is about six times smaller (82 MB installed instead of 483 MB), opens files faster, and switches pages in milliseconds. It also opens Excel files, writes Excel on export, and fixes several bugs in the expression language and in join and append.

**Q: Can I create my own themes?**
A: Yes. Export a theme as a template, edit the JSON, and import it back. See [docs/THEMES.md](docs/THEMES.md).

## Contact
- [Telegram](https://t.me/aziz_nadirov)
- [GitHub](https://github.com/AzizNadirov/ParVu)

## It has bugs

Yes, it has. Please [open an issue](https://github.com/AzizNadirov/ParVu/issues) and let's solve it.
