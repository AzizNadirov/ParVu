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
| `ops.rs` | `Operation`: every table operation (sort, rename, join, drop duplicates…), building its SQL and step label |
| `page.rs` | A page of results, formatted to text with Arrow's formatter |
| `export.rs` | Export via DuckDB `COPY`, or `rust_xlsxwriter` for Excel |
| `stats.rs`, `search.rs` | Column statistics, distinct values, samples, full-table search |

### How a tab is stored

Opening `sales.parquet` creates:

- `__pv_src_sales`: a view reading the file (for Excel, reading the table `__pv_xlsx_sales` loaded from it);
- `sales`: the view users query. Without edits it equals the source view. With pending cell edits it overlays them by row number, so queries, joins and saves see the edits without rewriting the file;
- a `QueryEngine` whose current query starts as `SELECT * FROM "sales"`.

Every applied step (query, operation or cell edit) goes into one list in the tab, so undo always reverts the most recent step. Saving exports the current query to a temporary file next to the target, renames it into place, and reopens the tab from the result.

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

Strings shown to users always go through `t(key)`. Colours and fonts come only from the `--pv-*` theme variables.
