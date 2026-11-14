
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
- **Built-in functions** - text (`UPPER`, `SPLIT_PART`, `LEFT`, `LPAD`, `REGEX_EXTRACT`…), dates (`YEAR`, `QUARTER`, `DATE_TRUNC`, `STRFTIME`…), math (`ROUND`, `LN`, `MOD`…), conditionals (`IF`, `IFNULL`, `GREATEST`), and table functions `DROP_DUPLICATES`, `DROP_NULL`, `REPLACE`
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

### Reshape and Analyse (pandas-style)
- **Group by** with count, sum, mean, median, min, max, std, var, first, last, most frequent, distinct count or list (`Ctrl+G`)
- **Pivot table** (one column per value, or a crosstab of counts), **unpivot** (melt) and **transpose**
- **Value counts** and **describe** (min, max, mean, std, quartiles and nulls of every column)
- **Filter rows** by conditions joined with AND or OR: =, ≠, <, >, between, contains, starts/ends with, regex, one of, is null or blank (`Ctrl+Shift+L`)
- **Fill missing values** with a value, the previous or next value, or the column's mean, median or most frequent value
- **Sort by several columns**, **choose and reorder columns**, keep the **first, last or a random sample** of rows
- **Split a column** by a separator (Text to Columns) and **bin values** into equal-width, equal-count or custom intervals (`cut` / `qcut`)
- **Running totals, ranks and rolling values**: cumulative sum/min/max/mean/count, shift, diff, percent change, rank (5 tie methods), rolling mean/sum/min/max/std, per group or overall

### Spreadsheet Features
- **Charts** - bar, line, scatter and histogram, calculated by DuckDB so they work on huge files (`Ctrl+Shift+C`)
- **Conditional formatting** - colour scales and highlight rules per column
- **Freeze columns**, remembered column widths, and **fill down** (`Ctrl+D`)

### Search
- **Search** (`Ctrl+F`) finds matches across every page, not just the visible one. Search one column or all, with match case, whole cell and regex options. `F3` / `Shift+F3` step through matches, and the table jumps to the right page and cell.

### Export
- CSV, TSV, Parquet, JSON, JSON Lines and Excel
- Pick columns, cast types, and format dates
- CSV delimiter, header and quoting; Parquet compression
- Progress bar and cancel for large exports

### AI clients (MCP)
- **`parvu mcp`** turns ParVu into an MCP server, so Claude Code, Claude Desktop, VS Code and other AI clients can open your data files and query them with DuckDB:
  ```bash
  claude mcp add -s user parvu -- parvu mcp --allow-dir ~/Documents
  ```
- Only folders you allow are reachable, queries are read-only, operations change only the session's copy, and nothing is written unless you add `--allow-write`. See [docs/MCP.md](docs/MCP.md).

### User Interface
- **Themes** - Material Design 3 interface with 5 built-in themes (Material Light, Material Dark, ParVu Light, Excel, ParVu Black), plus custom themes you can import and export
- **Languages** - English, Russian and Azerbaijani, switchable at any time
- **Multiple tabs and windows** (`Ctrl+N` opens a new window)
- **Shortcut cheatsheet** - `Ctrl+/`
- **Exit warning** for unsaved edits and transforms
- **Recent files** and a crash reporter

## Installation

### For End Users

- **Linux (Ubuntu/Debian)**: download the `.deb` package from [Releases](https://github.com/AzizNadirov/ParVu/releases) and install it
  ```bash
  sudo apt install ./parvu_<version>_amd64.deb
  parvu
  ```
  or let the installer script download it, or build it from source with every dependency:
  ```bash
  curl -fsSL https://raw.githubusercontent.com/AzizNadirov/ParVu/main/install.sh | bash -s -- --prebuilt   # latest release
  curl -fsSL https://raw.githubusercontent.com/AzizNadirov/ParVu/main/install.sh | bash                    # build from source
  ```
- **Windows**: download and run `ParVu_<version>_x64-setup.exe`. It adds a Start menu entry and file associations.

See [RELEASES.md](RELEASES.md) for details.

### For Developers

Requirements: [Rust](https://rustup.rs/) 1.88 or newer and Node.js 22.12 or newer. On Debian/Ubuntu also:

```bash
sudo apt install build-essential libwebkit2gtk-4.1-dev libsoup-3.0-dev \
     libjavascriptcoregtk-4.1-dev libgtk-3-dev librsvg2-dev
```

`./install.sh` installs all of these (Rust and Node.js only if missing or too old), builds the `.deb` and installs it; `./install.sh --beta` does the same for `parvu-next`.

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
cargo test -p parvu-core -p parvu-dsl -p parvu-config --features parvu-core/schema
cargo test -p parvu-app          # MCP server
npm --prefix ui test
```

Tests and development builds use the prebuilt DuckDB library. Installer builds compile DuckDB in from C++ source, which takes several minutes the first time; `build.sh` limits parallel jobs to fit the free memory.

| Path | Contents |
|------|----------|
| `crates/parvu-core` | DuckDB engine: tabs, cell edits, operations, charts, export, statistics, search, read-only access for AI tools |
| `crates/parvu-dsl` | Expression language parser and compiler |
| `crates/parvu-config` | Settings, recent files, translations, themes |
| `crates/parvu-app` | Tauri app: windows and commands; the `parvu mcp` server |
| `ui/` | React UI |

See [docs/BUILDING.md](docs/BUILDING.md) and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for more.

## How to Use

### Launch

```bash
parvu
parvu path/to/file.parquet another.csv
parvu mcp --allow-dir ~/data      # MCP server for AI clients, no window
```

### Basic Workflow

1. **Open a file** - drop it onto the window, use File → Open (`Ctrl+O`), or paste a path
2. **Browse** - use the pagination bar or type a page number
3. **Query** - write SQL in the editor and press **Execute** or `Ctrl+Enter`. The table is named after the file (`sales.parquet` → `sales`)
4. **Expression mode** - switch the editor to Expression and write, for example:
   - `sales[discount] = sales[price] * 0.1` to add a computed column
   - `DROP_DUPLICATES(sales[id], sales[name], 'first')` to deduplicate
5. **Data and Reshape menus** - filter rows, fill missing values, sort by several columns, group by, pivot, unpivot, value counts, describe; Operations has running totals, ranks and rolling values
6. **Column menu** - right-click a header to sort, filter, see value counts, a chart or statistics, fill missing values, transform (rename, change type, split, bin…), freeze columns or add conditional formatting
7. **Cell menu** - right-click cells to copy them as TSV, CSV or Markdown, filter by a value or a condition, fill down, or set a value to NULL
8. **Edit** - type into a cell; edits are highlighted and listed as steps
9. **Undo** - `Ctrl+Z` or the Undo button in Applied Steps
10. **Save** - `Ctrl+S` writes edits and transforms to the file; Applied Steps clears afterwards
11. **Export** - File → Export (`Ctrl+E`)
12. **Search** - `Ctrl+F` searches every row on every page
13. **Charts** - View → Chart (`Ctrl+Shift+C`)
14. **Theme and language** - View → Theme / Language

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

**Memory.** Queries may use up to 1 GB of working memory by default (File → Settings → Performance). Larger sorts, joins and groupings spill to a temporary folder on disk instead of using more RAM, so even multi-gigabyte files stay within that budget. The `PARVU_MEMORY_LIMIT_MB` environment variable overrides the setting.

### Themes

- **Material Light** - default; Material Design 3 light colours with Roboto
- **Material Dark** - Material Design 3 dark colours
- **ParVu Light** - the original light theme
- **Excel** - Excel-inspired green theme
- **ParVu Black** - dark theme

Every theme uses the same Material components (buttons, fields, dialogs, menus), drawn in the theme's colours. Custom themes live in `~/.ParVu/themes`. See [docs/THEMES.md](docs/THEMES.md).

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

**Q: Can I use it with AI?**
A: Yes, through MCP: `parvu mcp` lets Claude Code, Claude Desktop, VS Code and other MCP clients open and query your files with ParVu's tools, using your own AI subscription. See [docs/MCP.md](docs/MCP.md). A built-in assistant is planned ([plans/AI_PLAN.md](plans/AI_PLAN.md)).

**Q: Can I create my own themes?**
A: Yes. Export a theme as a template, edit the JSON, and import it back. See [docs/THEMES.md](docs/THEMES.md).

## Contact
- [Telegram](https://t.me/aziz_nadirov)
- [GitHub](https://github.com/AzizNadirov/ParVu)

## It has bugs

Yes, it has. Please [open an issue](https://github.com/AzizNadirov/ParVu/issues) and let's solve it.
