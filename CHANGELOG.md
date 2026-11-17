# ParVu Changelog

## [Unreleased]

## [1.0.0-alpha.6]

Operations are grouped by what they touch (Rows, Column, Table), and table operations open their result in a new tab. Column formulas speak about 100 Excel functions, Excel workbooks keep all their sheets, conditions follow the column's type and can limit a change to some rows, and big files can no longer take all of the computer's memory.

### Added
- **Conditions follow the column's type.**
  - In **Filter Rows** (and everywhere conditions are written), the operators fit the column: no "contains" for numbers, and yes/no columns get = / ≠ / empty.
  - The value box fits too: a number field, a date picker, date and time, time, or a yes/no choice.
  - Text columns suggest their most frequent values, read from the first 200,000 rows so it stays quick on big files.
- **Change only some rows.** Replace, Fill missing values and the column functions with options (prefix, extract, date formats…) have an **Only rows where…** section. Other rows keep their value; a new column is empty there. The step says which rows it touched, for example "… where region = north".
- **Set Value…** (column menu → Clean) writes one value, or empty, into a column on the rows that match conditions, like pandas `df.loc[condition, column] = value`.
- **Duplicates:**
  - **Drop Duplicates** can also keep **none**: every row that has a duplicate goes, as pandas `keep=False` does.
  - The new **Mark Duplicates** adds a yes/no column instead of dropping rows, like pandas `duplicated()`: every repeat after the first, before the last, or every duplicated row.
  - Both are in **Rows** and in the column menu's **Clean** group, with that column already ticked.
- **A slimmer query panel.** Collapsed, it's one line: the title, then Table Info, Filter, Group by, Chart, Formula, Join and Append. The SQL / Expression switch shows only while it's open. The menu-bar titles (File, Edit, View…) have icons, and the column menu's Type group uses a `dt` icon.
- **Menus with icons, and shorter right-click menus.**
  - Every menu item has an icon.
  - The column menu, both the right-click menu on a header and Column in the menu bar, is grouped into named submenus: **Column** (rename, duplicate, remove), **Type**, **Transform**, **Clean**, **Analyze**, plus the column type's own functions (Text, Extract, Dates…). The header menu keeps Sort and Filter at the top and adds **New Table from This Column** and **Display** (freeze, formatting).
  - In the cell menu, the extra copy formats moved into **Copy As**.
- **The SQL Query panel can be collapsed** to its title bar with the arrow next to its title, or with **View → Collapse SQL Query Panel**. It starts collapsed, as does Applied Steps; Reset Query shows only when the panel is open, and **Table Info** sits with Filter, Group by, Chart, Formula, Join and Append.
- **View choices are remembered:** whether the SQL Query panel is shown, collapsed and how tall it is, Applied Steps, and the AI assistant panel. They're also in **Settings → General → View**.
- **Table operations open their result in a new tab.** Group By, Pivot, Unpivot, Transpose, Value Counts, Describe, Join and Append no longer replace the table you run them on. The source keeps its data, steps and edits.
  - **The new tab is named after what happened:** `orders_by_region`, `orders_pivot`, `orders_join_customers`…
  - **It's a table of its own:** cells can be edited, formula columns added, and any operation applied to it.
  - **A chip by the table name shows where it came from:** `from orders · Group By region`. Click it to go to the source; **↻** computes the result again from the source's current data.
  - **It's not a file yet:** it shows the unsaved dot, `Ctrl+S` opens Save As, and closing it asks first.
  - **The result is computed in the background** with progress and Stop. A result over 5,000,000 rows isn't kept in memory: ParVu offers to write it straight to a Parquet or CSV file and open that.
  - **SQL:** the new **Result in new tab** box next to Execute Query runs a query into a new tab.
  - **AI clients** get the same rule: a table operation opens a new table, named in the answer, and the approval card says so.
- **Menus by what an operation touches: Rows, Column and Table.** They replace Data, Reshape and Operations.
  - **Rows:** filter, sort, first/last/random rows, drop duplicates, drop empty rows.
  - **Column:** names the selected column (`Column: qty`) and holds everything that adds or changes a column: formulas, rename, duplicate, remove, types, text and date functions, replace, fill missing, split, merge, bin, running totals. A column header's right-click menu is the same menu, plus sort, filter, freeze and formatting.
  - **Table:** group by, pivot, unpivot, transpose, value counts, describe, join and append.
  - **Fill** (down, up, right, left, Series…) moved to **Edit**.
- **Excel functions.** About 100 of Excel's most used functions work in formulas and expressions under their Excel names and argument order: `ROUNDUP`, `MROUND`, `IFS`, `SWITCH`, `IFERROR`, `PROPER`, `TEXTJOIN`, `TEXTBEFORE`, `DATEDIF`, `EOMONTH`, `NETWORKDAYS`, `AVERAGE`, `MEDIAN`, `LARGE`, `COUNTIF`, `SUMIFS`, `MAXIFS`, `XLOOKUP`, `VLOOKUP`, `TEXT` with Excel format codes, and more.
  - Conditional totals take Excel's criteria (`"North"`, `">10"`, `"a*"`), and `COUNTIF([region], [region])` counts the rows like this one.
  - `XLOOKUP` and `VLOOKUP` look up values in another open tab.
  - A single `=` compares, as in Excel: `IF([region] = "North", 1, 0)`.
  - **Help → Expression Language Guide** has a new **Excel functions** tab: every function by category with an example, and for the ones that don't fit a formula (INDEX, FILTER, OFFSET…) the menu that does the job. The guide has many more examples.
- **Excel workbooks with several sheets.** A workbook opens on its first visible sheet, and a **Sheet: … ▾** button next to the table name lists every sheet (it lives in the table's header, so sheets never look like the tabs at the bottom):
  - **Show in This Tab** switches the tab to another sheet; **Open in New Tab** and **Open All Sheets as Tabs** show sheets side by side (as tables `book_q2_2024`…, ready to join).
  - **Rename, Hide / Unhide, Duplicate, Move Left / Right, Delete** change the workbook's sheets. Like cell edits, they are saved with the workbook (`Ctrl+S`); until then a deleted sheet can be restored, and the tab shows the unsaved dot.
  - "Read as" choices and formulas are remembered per sheet.
  - AI clients can open a given sheet (`open_file` takes `sheet` and lists the sheets).
- **Column formulas and the formula bar.** A formula column computes every row from an expression such as `= [qty] * [price]`.
  - **Starting a formula:** **Column → New Formula Column** (or the same item in a column header's menu), the **Formula** button, or the **+** header after the last column. The formula bar opens above the grid while you write, and closes when you apply or cancel. Typing `=` in a cell is just a value.
  - **Writing it:** the bar has the expression language's highlighting and autocomplete. Each column the formula refers to gets its own colour, both in the bar and as a tinted column in the grid, and clicking a column header inserts it.
  - **Live, like Excel:** change a cell and the formulas using it update at once. Cell editing stays available, and undo covers adding, changing and removing a formula.
  - Formula columns show an *fx* icon. Their cells open the formula instead of being edited directly.
  - Saving writes the computed values.
  - **Column totals:** `= [price] / SUM([price])` divides by the column's total. Totals are computed once, and again after a cell edit, so paging stays fast.
  - **Ranges across a row:** `= SUM([q1:q4])` adds the columns q1 to q4 of each row (also AVG, MIN, MAX, COUNT). Shift+click a second column header in the formula bar to make a range; the grid tints every column in it.
  - **Neighbouring rows:** `= [total] - PREV([total])`, `NEXT([x], 2)`, in the file's row order, on tables up to 2 million rows.
  - Errors are underlined at their position in the formula bar, and Table Info lists each formula.
  - **Remembered per file:** a file's formulas come back when you open it again, as long as the file hasn't changed (like "Read as" choices).
  - **AI clients can add formulas:** the new `set_formula` tool (MCP and the built-in assistant) proposes a formula column on an approval card with a preview; nothing changes until you approve it.
- **Filling, as in a spreadsheet.** Drag the square at the corner of a selection to fill the cells next to it, down, up, left or right. ParVu continues the series instead of copying:
  - numbers by their step (1, 2 → 3, 4; one number is copied);
  - dates by day, or by month or year when the seeds share a day of the month (31 Jan, 29 Feb → 31 Mar keeps to month ends); timestamps keep their format;
  - month and day names in English, Russian and Azerbaijani (Jan → Feb, пн → вт), keeping their case;
  - text ending in a number (Item 1 → Item 2, INV-007 → INV-008);
  - anything else repeats the selected cells.
  - **Series…** (Edit → Fill, or the cell menu) fills from the first selected cell with a step you choose: linear, growth, or dates by days, weeks, months or years, with an optional stop value.
  - **Fill Down, Up, Right and Left** in the Data menu and the cell menu copy the selection's edge row or column; Ctrl+D fills down and Ctrl+R fills right. Every fill is one step that undo reverts. Formula columns are left as they are.

### Fixed
- **Saving an Excel workbook kept only one sheet.** `Ctrl+S` on a workbook rewrote it with the sheet shown and silently dropped the others. Now every sheet is written, with its name and hidden state. ParVu writes values, so cell formatting, Excel formulas and charts are not kept; it says so before overwriting a workbook and offers **Save As a Copy…**.
- **Saving an old `.xls` file overwrote it with CSV text.** ParVu can't write `.xls`, so Save now refuses and points to Save As `.xlsx`.
- **ParVu no longer opens whenever Claude Code (or VS Code with Claude) starts.** MCP clients start their servers when a session opens, and `parvu mcp --attach` used to open ParVu right then. Now it answers the client's startup itself and opens ParVu only when the AI first uses a tool.
- **Show Unique Values no longer freezes on big files.** It used to compute every distinct value of the column at once, holding the window until it finished. The column is now read piece by piece:
  - the dialog opens at once and fills as values are found, with progress and Stop;
  - a column with many values answers after its first rows (the first 10,000 values found, with a note);
  - memory stays small whatever the file's size.
- **Long work keeps the window usable.** Column statistics, charts, colour-scale ranges, search, Copy as Tuple, the Convert dialog's full count and the JSON key scan now run beside the grid, not in front of it. Each shows its progress with a Stop button (in its dialog, or in the status bar), and a newer request replaces an older one instead of queueing behind it.
- **Less memory on big tables:**
  - Statistics estimate the distinct count (marked ≈) on tables over 2 million rows.
  - Copy as Tuple → random samples without sorting the table, at most 100,000 values.
  - Colour scales read all their columns in one pass.
- **A query can no longer take all of the computer's memory.** A big file once made the whole system run out of memory.
  - Whole-table totals (statistics, formula totals such as `MODE`, Group By with no columns, Fill missing with mean, median or most frequent) are now computed in a form that DuckDB can spill to disk.
  - A memory guard watches ParVu's memory. If it passes twice the query memory limit plus 1 GB (at most 65% of the computer's memory) and keeps growing, the running queries stop with "This operation needed more memory than allowed". The guard also covers `parvu mcp`; `PARVU_MEMORY_GUARD_MB` sets the ceiling, `0` turns the guard off.
  - On Linux, if memory runs out anyway, the system stops ParVu before other programs.
- **Filtering a big file failed with "Out of Memory Error: could not allocate block".** Each DuckDB thread reading a wide Parquet file needs 100–150 MB, and 8 threads did not fit in the 1 GB query limit. ParVu now uses one thread per 256 MB of the limit (4 at the default), and a page that still runs out of memory is read again on one thread. Raising the limit in Settings brings back more threads.
- Changing settings or the language no longer waits for a running query.
- Stop in the Convert dialog's full count works again after changing the reading.

## [1.0.0-alpha.5]

Column types are now detected in Parquet, JSON and Excel files too, not just CSV: text columns that hold dates, numbers or yes/no values get those types when the file opens.

### Fixed
- **Dates and other types in Parquet, JSON and Excel files.** Detection on open used to run only for CSV files, so a Parquet column of dates stored as text stayed text. Now the text columns of every format go through the same detection on their first 10,000 rows. The file's own types stay as they are, and a column that reads as dates, numbers, yes/no or JSON gets that type. **Read as** and the day/month flip work for these files too.
- **A text column mixing dates and timestamps** (`2026-07-01` next to `2026-07-01 12:30:00`) is read as timestamps. Before, it stayed text.

## [1.0.0-alpha.4]

Columns that know their type, in the spirit of Power Query: convert with options and see what doesn't convert, and use the operations that fit each column (text, yes/no, dates, JSON). Also: open CSV files with a delimiter, header, skipped lines, decimal comma or encoding.

### Added
- **Convert a column** (column menu → Change Type → Convert…): read a column as a number written `1,234.5`, `1.234,5` or `1 234,5`, yes/no, a date in one of several formats, date and time, time, JSON, or Unix time in seconds, milliseconds or microseconds.
  - ParVu suggests the reading that fits the column.
  - A live preview shows the values before and after, and marks the ones that don't convert.
  - The dialog counts the values that don't convert in the first 100,000 rows, and can count all rows (the count can be stopped).
  - Values that don't convert become empty, or a value you choose. The original column can be kept next to the new one.
  - **Show rows that don't convert** keeps just those rows, so you can fix them (Power Query's Keep Errors).
- **Text columns** get **Text ▸**, **Extract ▸** and **Merge columns…** in their column menu:
  - **Text ▸** (one click): trim (both ends, start or end), remove non-printable characters, collapse repeated spaces, lowercase, UPPERCASE, Capitalize Each Word, remove accents. Add prefix…, add suffix… and pad… open a small dialog.
  - **Extract ▸**: length, first or last characters, a range of characters, text before or after a delimiter (first or last occurrence), text between two delimiters, a regular expression match. The result goes to a new column right after the source, or into the same column.
  - Each dialog shows a live preview over values from the page.
  - **Merge columns…** (also in the Data menu) joins several columns with a separator, skipping empty cells.
- **Yes/no columns** get **Invert**, **Keep yes**, **Keep no** and **Keep empty**.
- **Date and time columns** get these entries in their column menu:
  - **Date ▸**: year, quarter, month, month name and day name (in the interface language), day, day of week, ISO week, day of year, is weekend.
  - **Time ▸**: hour, minute, second, date only, time only.
  - **Start of ▸** and **End of ▸** a period. Date columns stay dates.
  - **Age until today** in days, months or years.
  - **Add or subtract time…**
  - **Format as text…**, with format presets.
  - **Difference from another date…**, in whole days, months, years and so on.
  - **Filter dates ▸**: last 7 or 30 days, this or the previous week, month, quarter or year, year to date, or the last N days, weeks, months, quarters or years.

  "Today" is evaluated when the step runs, so "last 7 days" keeps meaning that. The filters are plain date ranges, so Parquet files skip the row groups outside them.
- **JSON, list and struct columns:**
  - **Expand into columns…**: choose keys of a JSON object column. They're found in the first 2,000 rows, or in all rows on request, and each becomes a typed column at the JSON column's place.
  - **Extract a value…**: a JSON path such as `$.address.city`.
  - **Items to rows**: rows whose list is empty are kept.
  - **Number of items** and **Join items into text…**.
  - Struct columns, for example from JSON files: **Expand into columns** in one click.
- **File → Open with Options…** for CSV files: delimiter, whether the first row holds the column names, lines to skip at the top, decimal comma, and encoding (UTF-8, UTF-16, Latin-1).
  - The dialog shows what DuckDB detects on its own and a preview of the first rows.
  - ParVu remembers the options for that file, so opening it again (also from Recent files) uses them, until the file changes.
- **Text ▸ Split into rows…** (one row per piece), and **Extract ▸ Contains (yes/no)**: contains, starts with, ends with, or matches a regular expression, optionally ignoring case.
- **Expression functions:** LTRIM, RTRIM, STRIP_ACCENTS, CONCAT_WS, POSITION, TRY_STRPTIME, MONTHNAME, DAYNAME, LAST_DAY, EPOCH, JSON_EXTRACT, JSON_EXTRACT_STRING.

## [1.0.0-alpha.3]

AI comes to ParVu: use your Claude subscription through Claude Desktop or Claude Code, or chat in the new assistant panel with an API key or a local model. Every change the AI wants to make needs your approval. Also: smarter charts, and CSV columns that get the right types (with a fix for files that broke on a bad value far down).

### Added
- **MCP server:** `parvu mcp --allow-dir <folder>` lets AI clients (Claude Code, Claude Desktop, VS Code) open data files and query them through ParVu, with read-only SQL checks and session-only changes. See [docs/MCP.md](docs/MCP.md).
- **AI assistant panel** (`Ctrl+Shift+A`): ask about your tables with an Anthropic, OpenAI, Gemini, OpenRouter or Groq API key, or a local Ollama / LM Studio model. It inspects and queries the data with the same read-only tools as the MCP server; changes go through approval cards. Keys are stored in the system keychain. See [docs/AI.md](docs/AI.md).
- **AI → Connect an AI client:** Claude Desktop, Claude Code and other MCP clients connect to the running window (`parvu mcp --attach`) and work with its open tabs, using your own Claude subscription. Their changes appear as approval cards with a preview and become normal undoable steps. The status bar shows connected clients. If ParVu isn't running, `parvu mcp --attach` opens it.
- **Column types for CSV files.** ParVu checks DuckDB's types against its own detection on the first 10,000 rows and reads more than DuckDB does:
  - numbers like `1.234,56`, `1 234,5` or `1,234.5`;
  - yes/no words (also in Russian and Azerbaijani);
  - a second date format in the same file;
  - JSON.

  Columns DuckDB already types correctly keep its fast reader.
- **Type icons in the column headers** (number, text, yes/no, date, time, JSON…). Dates such as `03/04/2024`, where the day and month order is a guess, show a calendar with a question mark.
- **Read as** in the column menu of a CSV file: read a column as text, a whole number, a number with `,` or `.` decimals, yes/no, a date in one of several formats, a time or JSON. "Read dates day first / month first" flips a guessed date column in one click. The change is a step you can undo, cell editing stays available, and ParVu remembers it for that file (until the file changes).
- **Settings → Column types:** turn type detection off, and choose how dates such as `03/04/2024` are read (automatic from the language, day first or month first).

### Fixed
- **A bad value far down a CSV file no longer breaks the table.** DuckDB guesses types from the start of the file, so a value like `oops` in a number column 100,000 rows down used to make every sort, sum, chart or export fail. ParVu now re-reads that column safely, and the value becomes empty.
- **Change type** no longer breaks the table when a value doesn't convert: the value becomes empty instead of making a later page, sort or export fail. (ParVu also turns off DuckDB's `type_pushdown` optimizer, which made this safe conversion strict for CSV columns.)
- **Export** with a per-column type writes values that don't convert as empty instead of stopping halfway with a partial file.
- **Split column** without a number of parts counts them on the first 10,000 rows instead of reading the whole table first.
- `install.sh` installs a fresh build even when its version matches the installed one (before, apt kept the old program).
- Windows installer builds in CI: the prebuilt DuckDB library is downloaded again before each build.

### Changed
- **Charts:** a suggested chart for the selected column; area charts; dates grouped by day, week, month, quarter or year; top N with the rest as "Other"; series split by a category (stacked or grouped); readable axes (round ticks, compact numbers, round histogram intervals); hover tooltips, a legend, a table view; PNG and SVG export; colours that hold up in dark themes and for colour-blind readers.

## [1.0.0-alpha.2]

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
- `install.sh`: installs ParVu on Debian and Ubuntu, either the latest release (`--prebuilt`, checked against `SHA256SUMS.txt`) or built from source with all build dependencies.

### Fixed
- Publishing a GitHub release (installer files were not found after download); a release can be re-run for an existing tag.
- Windows installer builds in CI failing with `cannot open input file 'duckdb.lib'` after a cache restore.

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
