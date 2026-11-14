# ParVu Architecture

ParVu is a Tauri desktop app. A React UI runs in the system webview and calls Rust commands. All data work happens in Rust with DuckDB.

```
┌──────────────────────────────────────────────┐
│ ui/  (React, TypeScript)                     │
│   App.tsx, grid, editor, dialogs             │
│   lib/api.ts  → invoke("command", args)      │
├──────────────────────────────────────────────┤
│ crates/parvu-app  (Tauri)                    │
│   commands.rs: one function per command      │
│   state.rs: one Workspace per window         │
├────────────────┬───────────────┬─────────────┤
│ parvu-core     │ parvu-dsl     │ parvu-config│
│ DuckDB engine  │ expressions   │ settings,   │
│ tabs, edits,   │ → SQL         │ themes,     │
│ ops, export    │               │ translations│
└────────────────┴───────────────┴─────────────┘
```

Dependencies only point down. `parvu-dsl` and `parvu-config` don't depend on DuckDB, and none of the three library crates know about Tauri.

## parvu-core

| Module | Purpose |
|---|---|
| `workspace.rs` | `Workspace`: one DuckDB connection and the open tabs of a window. Opening, steps, undo, edits, save |
| `engine.rs` | `QueryEngine`: a tab's current SQL, undo history and paging |
| `format.rs` | File formats; registering a file as a DuckDB view; reading Excel with `calamine` |
| `edits.rs` | Pending cell edits and the overlay SQL that applies them |
| `ops.rs` | `Operation`: every table operation, building its SQL and step label; the simple ones are built here |
| `reshape.rs` | Group by, pivot, melt, transpose, value counts, describe |
| `clean.rs` | Condition filters, fill missing values, multi-column sort, choosing columns, head/tail/sample, split, bin |
| `window.rs` | Running totals, shift/diff, rank and rolling values, per group |
| `chart.rs` | Chart data aggregated or sampled by DuckDB |
| `page.rs` | A page of results, formatted to text with Arrow's formatter |
| `export.rs` | Export via DuckDB `COPY`, or `rust_xlsxwriter` for Excel |
| `stats.rs`, `search.rs` | Column statistics, distinct values, samples, full-table search |

### How a tab is stored

Opening `sales.parquet` creates:

- `__pv_src_sales`: a view reading the file (for Excel, reading the table `__pv_xlsx_sales` loaded from it);
- `sales`: the view users query. Without edits it equals the source view. With pending cell edits it overlays them by row number, so queries, joins and saves see the edits without rewriting the file;
- a `QueryEngine` whose current query starts as `SELECT * FROM "sales"`.

Every applied step (query, operation or cell edit) goes into one list in the tab, so undo always reverts the most recent step. Fill down is one step holding several cell edits and the earlier edit of each cell, so undo restores them exactly.

Operations whose SQL depends on the data (pivot columns, bin edges, split widths, transpose names) read it through `OpContext::query` before building their SQL. Row order is kept with a `ROW_NUMBER() OVER ()` column (`__pv_rn`) wherever an operation needs it.

Conditional formatting, frozen columns and column widths belong to the window (`TabUi` in `App.tsx`), not to the data: they are not steps and are never saved. Saving exports the current query to a temporary file next to the target, renames it into place, and reopens the tab from the result.

### Memory

Each window's DuckDB connection is limited to 8 threads and to the **query memory limit** setting (1 GB by default, `PARVU_MEMORY_LIMIT_MB` overrides it). Above the limit DuckDB spills to `~/.cache/ParVu/duckdb`. Freed memory is returned to the system after large queries. Pages are fetched with `LIMIT/OFFSET`, so the first pages of a sorted result only need a top-N, while jumping deep into a sorted big file needs a full sort. A sort applied right after another sort replaces it rather than wrapping it, so the data is sorted once. A tab's column list is cached until its query changes.

In the UI, the grid reads theme colours once per theme, reuses its style objects, and only re-renders when the page, sort or theme changes. Dialogs are loaded on first use.

### Rules

- All identifiers and literals go through `sql::quote_ident` and `sql::quote_literal`.
- Type names used in `CAST` are checked with `sql::is_safe_type_name`.
- New table operations are new `Operation` variants, so the UI, undo and step labels pick them up in one place.

## parvu-dsl

```
text → parse (pest grammar) → resolve (AST + catalog → IR) → compile (IR → DuckDB SQL)
```

`translate()` is what the editor runs: it returns the SQL and whether the expression is an assignment, a table-level query (`DROP_DUPLICATES`, `DROP_NULL`) or a scalar. `DROP_DUPLICATES`, `DROP_NULL` and `REPLACE` are handled in the resolver before the function registry; adding them to the registry only enables autocomplete. The golden file `tests/golden/dsl.json` pins the output for 306 expressions.

## parvu-config

Settings, recent files, themes and translations, all stored in `~/.ParVu` in the same format the Python app used. Themes are turned into CSS variables (`--pv-<group>-<field>`), and the UI styles itself only through them. See [THEMES.md](THEMES.md) and [I18N.md](I18N.md).

## parvu-app

- `main.rs` sets up logging (`~/.ParVu/logs`), the crash hook, plugins and windows.
- `commands.rs` holds every command the UI can call. Data commands run on the blocking thread pool so the window stays responsive.
- Each window has its own `Workspace`, created on first use and dropped when the window closes.
- Exports run on a second DuckDB connection in a background thread and report progress through `export-progress` and `export-finished` events; `cancel_export` interrupts DuckDB.

## ui

| Path | Purpose |
|---|---|
| `src/App.tsx` | The main window: menus, shortcuts, tabs, and calls to the backend |
| `src/lib/api.ts` | Typed wrappers for every command, and the shared types |
| `src/components/DataGrid.tsx` | Glide Data Grid (canvas, virtualised) |
| `src/components/QueryEditor.tsx` | CodeMirror SQL and expression editor |
| `src/dialogs/` | One file per dialog, each opened with `openXxx(ui, args)` |
| `src/lib/i18n.ts` | Translation lookup with Python-style `{name:,}` placeholders |

Strings shown to users always go through `t(key)`. Colours and fonts come only from the `--pv-*` theme variables and the Material Design 3 `--md-*` tokens derived from them in `ui/src/styles.css` (surface tints, outlines, state layers, elevation, shapes). `--pv-derived-on-accent` (black or white, computed in Rust) is the text colour on the accent colour.
