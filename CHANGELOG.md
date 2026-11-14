# ParVu Changelog

## [Unreleased]

### Added
- **Data menu:** filter rows by conditions (AND/OR), multi-column sort, choose and reorder columns, first/last/random rows, fill missing values (value, forward, backward, mean, median, most frequent), split a column, bin values.
- **Reshape menu:** group by with 14 aggregations, pivot table and crosstab, unpivot (melt), transpose, value counts, describe all columns.
- **Running totals, ranks and rolling values** (Operations menu): cumulative sum/min/max/mean/count, shift, diff, percent change, rank, percent rank, row number, rolling mean/sum/min/max/std, per group and in any row order.
- **Charts:** bar, line, scatter and histogram, aggregated by DuckDB.
- **Conditional formatting:** colour scales and highlight rules; **freeze columns**; column widths kept per tab; **fill down** (`Ctrl+D`) as one undoable step.
- **Expression functions:** `IFNULL`, `GREATEST`, `LEAST`, `SPLIT_PART`, `SUBSTR`, `LEFT`, `RIGHT`, `LPAD`, `RPAD`, `REVERSE`, `REGEX_EXTRACT`, `REGEX_MATCH`, `STRFTIME`, `STRPTIME`, `QUARTER`, `WEEK`, `DAYOFWEEK`, `HOUR`, `MINUTE`, `DATE_TRUNC`, `LN`, `LOG10`, `EXP`, `MOD`, `SIGN`.
- Shortcuts: `Ctrl+Shift+L` filter, `Ctrl+G` group by, `Ctrl+D` fill down, `Ctrl+Shift+C` chart.
- **Material Design 3 interface:** tonal, filled, outlined, text and icon buttons with ripples; outlined text fields; rounded dialogs, menus and cards with elevation; Material icons; app bar, tabs and progress bars in Material style. Works with every theme, including custom ones.
- **Material Light** (new default) and **Material Dark** themes, with Roboto and Roboto Mono bundled.

## [1.0.0-alpha.1]

ParVu is rewritten in Rust (Tauri window, React UI, DuckDB). It reads the same `~/.ParVu` settings, recent files and themes as 0.x, and the `.deb` replaces the Python package. The Python code has been removed. See `plans/RUST_REWRITE_PLAN.md`.

### Changed
- **Much smaller and faster.** 82 MB installed instead of 483 MB; files open in about 400 ms instead of 620–760 ms; switching a 1,000-row page takes about 8 ms instead of 190 ms.
- **Pending cell edits are visible to queries.** Edits are overlaid on the file view, so SQL, joins and saves see them before saving.
- **Every query is an applied step**, so undo always reverts the most recent change.
- **Bounded memory.** Queries use at most 8 threads and 1 GB of working memory by default (new **Performance** setting); larger operations spill to disk. Sorting and jumping to the last page of a 5-million-row file peaks around 800 MB instead of 1.3 GB, and memory is returned afterwards.
- **A new sort replaces the previous sort** instead of sorting the already sorted result again.
- **Languages and themes switch instantly**, without a restart.
- **Windows ships an NSIS `setup.exe` only**; the portable zip is gone.

### Added
- **Excel files** (`.xlsx`, `.xls`) open without an internet connection, and export and Save As write Excel with real date cells.
- **Export** to TSV and JSON Lines, with progress and cancel.
- **Live SQL preview** in expression mode, and autocomplete with function docs.
- **Go to page** box and first/last page buttons.

### Fixed
- **Expressions:** parentheses were dropped (`(a + b) * 2` computed `a + b * 2`), `FALSE` compiled to `TRUE`, and `-x` / `NOT x` crashed.
- **Join, append and filter by values** read the original file and discarded earlier steps; they now use each tab's current result.
- **Drop duplicates, keep last** failed because it ordered by `rowid`, which does not exist on a view.
- **An expression naming another tab** replaced the active tab's data; it is now refused with a message.
- **Reset** filled the editor with `FROM data` even when the table had another name.
- **Column names** are quoted everywhere in generated SQL.

### Removed
- The Python application, its tests and PyInstaller packaging.
- The Python plugin API (no plugins existed).

## [0.3.1]

### Added
- **Search dialog** (`Ctrl+F`, also under **Edit → Find...**) — searches every row in the active table, not just the visible page. Incremental Next/Prev navigation with on-demand DuckDB queries; previously visited matches are cached so re-navigation is instant. Column scope dropdown, `Match case` / `Whole cell` / `Regex` options. `Enter` triggers Next, `Shift+Enter` Prev. Status line reports `Match N of M` (with `+` suffix when more may exist) and "first / last match reached" when the table edge is hit. Selecting a match paginates to the right page and selects the cell (deferred until the async page-load completes so the navigation lands on the loaded data, not stale state).
- **Column Statistics popover** — right-click any column header → **Column Statistics...**. One DuckDB aggregate against the active query returns `rows / non-null / null (with %) / distinct / min / max`, plus `mean / std` for numeric-castable columns (`TRY_CAST(col AS DOUBLE)`). Modal dialog with key/value table and a **Copy** button (TSV to clipboard for paste into Slack/Jira). Respects applied transforms.
- **DropNull operation** — `Operations → Drop Null Values...` or column header context menu. Drops rows where the column is NULL by default, or where the column equals a sentinel value the user types (`-1`, `''`, `N/A`, ...). With a sentinel, real `NULL` rows are kept; chain another DropNull with no sentinel to remove them too. Also available as expression: `DROP_NULL(table[col])` or `DROP_NULL(table[col], -1)`.
- **Drag-and-drop file open** — drop one or many local files onto the window to open each as a new tab.
- **Sortable column headers** — click a header to sort ascending; click again to flip to descending. Sort indicator (▲/▼) shown in the header. Sort state is per-tab and restored on tab switch.
- **Find in current page** (inline find bar widget) — separate from the full-table dialog, kept available for programmatic use. Status: case-insensitive substring or whole-cell match across visible rows.
- **Copy as CSV / TSV / Markdown** — right-click any cell selection: `Copy` (TSV, default), `Copy as CSV`, `Copy as Markdown`, `Copy with Headers (TSV)`. `Ctrl+C` produces a rectangular TSV (gaps in non-rectangular selections are filled with empty strings).
- **Keyboard shortcuts cheatsheet** (`Ctrl+/`, also under **Help → Keyboard Shortcuts**) — modal listing every shortcut grouped by File / Table / Find / Help.
- **Edit menu** with `Find...` (Ctrl+F), `Find Next` (F3), `Find Previous` (Shift+F3).
- **Windows build improvements** — installer (`ParVu-<v>-setup.exe`) and portable archive (`ParVu-<v>-portable.zip`):
  - Portable archive now created via `tar.exe` (built into Windows 10+) instead of `Compress-Archive`, which OOMs on the ~300 MB PyInstaller bundle.
  - Installer's `Desktop Shortcut` and `Associate .parquet/.pq/.csv/.json files` tasks are **checked by default**.
  - New `docs/BUILD_WINDOWS.md` with direct Inno Setup 6.7.3 download link and the `build.ps1` switches (`-SkipInstaller`, `-SkipPortable`, `-NoClean`, `-Help`).
  - `parvu.spec` updated to bundle resources at `parvu/resources/` (matches `RESOURCES_DIR`) and use `src/parvu/__main__.py` as the entry point.

### Changed
- **Save now commits applied transforms**, not just cell edits. `Ctrl+S` writes the transformed result back to the file (via DuckDB `COPY` for transforms-only; via COPY-to-temp + pandas for transforms+edits; legacy pandas path for edits-only). After save, the Applied Steps panel clears and the tab's engine re-reads the saved file. No more "Confirm Close" dialog if you saved your transforms.
- **Menu icons** swapped for more appropriate Qt standard pixmaps: New Window (file), Export (file with link arrow), Recent Files (folder), Math (content view), Drop Duplicates (trash), Replace (reload), Expression Help (?). Added left padding to menu icons via `QMenu::icon { padding-left }`.

### i18n
- All new strings translated in English, Russian, Azerbaijani (~80 new keys): Edit menu, Search dialog, Column Statistics dialog, DropNull dialog/step, shortcuts cheatsheet, find bar, cell context menu, header context menu additions.
- `ConfirmCloseDialog` now uses the translator (existing `dialog.confirm_close.*` keys), no longer hardcodes English.

### Fixed
- **Search-jump page race** — when the search dialog jumped to a row on another page, cell selection ran against stale table state before the async `_load_page` finished. Selection is now deferred to `_on_page_loaded`.

## [0.3.0] - 2026-05-27

### Added
- **Excel-style bottom tab bar** — Custom-painted sheet tabs with rounded top corners, active accent line, close button on hover, and elided text. Includes a hand-drawn "+" AddButton for new tabs and hover tooltips showing the full tab name.
- **Collapsible Operations panel** — SQL/Expression editor, mode toggle, and query toolbar are now wrapped in a collapsible accordion panel to save vertical space for data viewing.
- **Accordion-style headers** — Both Operations and Applied Steps panels use styled `AccordionHeader` widgets with chevron arrows, hover highlighting, and a left accent bar when expanded.
- **Unsaved Changes dialog** — Replaced native `QMessageBox` with a custom `UnsavedChangesDialog` featuring proper singular/plural messages ("1 unsaved cell edit" vs "3 unsaved cell edits"), no minimize/maximize buttons, and consistent button labels across platforms.
- **Function Documentation Popup** — Expression editor shows a tooltip-style popup with function signature, parameters, description, example, and SQL mapping when navigating the completer or clicking a function name.
- **REPLACE function** — New DSL function and method for string replacement with support for literal/regex matching and case-insensitive mode.
  - `REPLACE(text, pattern, with_value, case_sensitive, regex)`
  - Also available as method: `data[name].replace('old', 'new', FALSE, FALSE)`
  - Compiles to DuckDB `REPLACE()` or `REGEXP_REPLACE()` with appropriate flags.
- **Confirm Close dialog** — Warns when closing with applied transforms. Options: Save & Close (exports results), Close anyway, Cancel. Includes "Do not ask again" checkbox with setting persistence.
- **Tab deduplication** — Opening the same file twice creates tabs with `_2`, `_3`, etc. suffixes instead of overwriting.
- **Copy Values as Tuple dialog** — For large tables, shows sampling options: Only this page, First N, Random N (N configurable up to 1000). Optional label prefix (e.g. `my_values = (1, 2, 3)`).
- **Button styling** — Smaller, rounded (8px radius), with 2px margin for a Material-like appearance.

### Changed
- **Operations layout tightened** — Removed the separate query label widget; the label now lives inside `QueryEditor`'s header row alongside the mode toggle, saving one full row of vertical space.
- **Assignment compilation** — Overwrites existing columns instead of creating duplicates (e.g. `brand` instead of `brand_1`).
- **Table qualifier stripping** — Math operations and Replace dialog now strip table qualifiers when generating subquery SQL to avoid "table not found" errors.
- **Method call grammar** — Parser now supports method arguments: `data[name].replace('a', 'b')`.
- **DROP_DUPLICATES registry** — First parameter corrected from `columns` to `table`.

### Fixed
- **QueryEditor vertical expansion** — Prevented the editor from stretching and consuming excessive vertical space. `QueryEditor` is now capped at 85px and uses `Maximum` vertical size policy.
- **Applied Steps empty state** — Capped the content height at 100px so the expanded panel doesn't grow unexpectedly when empty.
- **AddButton visibility in Excel theme** — Fixed the "+" button being invisible because `button_background` and `accent_primary` were the same color. Now uses `tab_inactive_background` for contrast.
- **Expression editor assignment** — Fixed issue where assigning to an existing column created a duplicate column.
- **Replace dialog SQL** — Fixed binder error when using table-qualified column references inside subqueries.

## [0.2.0] - 2025-12-29

### Added - i18n System
- **Internationalization (i18n) support** with 3 complete languages:
  - 🇬🇧 English (en) - Default
  - 🇷🇺 Russian (ru) - Русский - 180+ keys
  - 🇦🇿 Azerbaijani (az) - Azərbaycan - 180+ keys
- `src/i18n.py` - Core i18n system with Locale and I18n classes
- `src/language_selector.py` - Language selector widgets for settings
- Language preference saved in settings (`current_language` field)
- Translation system with variable formatting support
- Flag emoji display for language identification
- **Full UI integration** - All 4 main UI files translated (~95 strings)

### Added - Documentation
- `docs/I18N.md` - Complete i18n user and developer guide
- `docs/I18N_INTEGRATION_EXAMPLE.md` - Real code integration examples
- `docs/I18N_SUMMARY.md` - Implementation summary and progress
- `docs/README.md` - Documentation index and quick reference
- Organized all documentation into `docs/` directory

### Changed - Themes
- Updated all references in code and documentation
- Theme JSON file now saved as `parvu_black.json`

### Changed - Project Structure
- Moved all documentation to `docs/` directory:
  - `THEMES.md` → `docs/THEMES.md`
  - `MIGRATION.md` → `docs/MIGRATION.md`
  - `PROJECT_STRUCTURE.md` → `docs/PROJECT_STRUCTURE.md`
  - New i18n docs in `docs/`
- Updated README.md with language information and new doc links

### Enhanced - Settings
- Added `current_language` field to Settings model
- Language preference persists across application restarts
- i18n initialized on app startup with saved preference
- Language selector integrated into Settings → General tab
- **Working UI translations** - Change language and see immediate effect after restart

### Enhanced - Documentation
- Updated README.md with i18n features
- Added language FAQ section
- Improved documentation organization
- Added comprehensive guides for developers

## [0.1.0] - Previous Release

### Added - Theme System
- 3 built-in themes (ParVu Light, Excel, ParVu Black)
- Custom theme creation and management
- Import/export themes as JSON
- Full color and layout customization

### Added - Core Features
- Parquet, CSV, JSON file support
- Lazy loading for huge files (8GB+)
- SQL querying with DuckDB
- Pagination and table operations
- Syntax highlighting and auto-completion
- Export to multiple formats

---

## Version Comparison

### What's New in 0.3.0?

**Major Features:**
- 📑 Excel-style bottom tab bar with custom painting
- 📂 Collapsible Operations accordion panel
- 🔔 Custom Unsaved Changes dialog with proper pluralization
- 🔍 Function documentation popup in expression editor
- 🔁 REPLACE DSL function and dialog

**UI Polish:**
- Accordion-style headers with hover effects and accent bars
- Tighter Operations layout (label integrated into editor header)
- Fixed QueryEditor vertical expansion
- Fixed Applied Steps empty-state height
- Fixed AddButton visibility in Excel theme

**Fixes:**
- Assignment compilation now overwrites existing columns
- Table qualifier stripping in subqueries
- Method call grammar supports arguments

### What's New in 0.2.0?

**Major Features:**
- 🌍 Multi-language support (English, Russian, Azerbaijani)
- 📝 Full i18n integration across the UI
- 🎨 Enhanced theme system
- 📚 Comprehensive documentation

**Key Changes:**
- Language selector in settings
- Persistent language preference
- Updated documentation structure
- Theme improvements
