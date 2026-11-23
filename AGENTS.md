# ParVu — Agent Guide

How to find your way around ParVu and change it safely.

## Project at a glance

**ParVu** is a desktop app for viewing and querying Parquet, CSV, JSON and Excel files.

- **Backend:** Rust (edition 2024), DuckDB through the `duckdb` crate, in a Cargo workspace
- **Window:** Tauri 2 (system webview)
- **UI:** React 18 + TypeScript + Vite in `ui/`; Glide Data Grid for the table; CodeMirror 6 for the editors
- **User data:** `~/.ParVu` (settings, recent files, themes, logs), same format as the old Python app
- **Entry point:** `crates/parvu-app/src/main.rs` (the window), or `parvu mcp` (`src/mcp.rs`, an MCP server without a window)

The app was rewritten from Python/PyQt6; the plan and its status are in `plans/RUST_REWRITE_PLAN.md`. `plans/AI_PLAN.md` is the plan for AI. Done: the MCP server (`parvu mcp`, attached mode for the running window), its read-only data layer, and the built-in assistant panel. See `docs/AI.md` and `docs/MCP.md`.

## Layout

```
crates/parvu-core     DuckDB engine: tabs, paging, edits, operations, export, stats, search
crates/parvu-dsl      expression language: parser → resolver → compiler (no DuckDB)
crates/parvu-config   settings, recents, themes, translations (no DuckDB)
crates/parvu-ai       AI assistant: providers, agent loop, pricing (no DuckDB, no Tauri)
crates/parvu-app      Tauri app: commands.rs, state.rs, main.rs, mcp.rs, packaging
ui/src                React UI: App.tsx, components/, dialogs/, lib/
docs/                 user and developer guides
```

Dependencies only point down: `ui → parvu-app → (parvu-core, parvu-dsl, parvu-config)`. Keep DuckDB out of `parvu-dsl` and `parvu-config`, and Tauri out of all three library crates. More detail in `docs/ARCHITECTURE.md`.

## Commands

```bash
cargo test -p parvu-core -p parvu-dsl -p parvu-config --features parvu-core/schema   # Rust tests (prebuilt DuckDB, seconds)
cargo test -p parvu-ai -p parvu-app                        # AI assistant and MCP server tests
cargo clippy --workspace --all-targets -- -D warnings      # must stay clean
npm --prefix ui run typecheck && npm --prefix ui test      # UI
cd crates/parvu-app && npx --prefix ../../ui tauri dev     # run the app
./build.sh                                                 # installer
```

**Memory warning:** everyday builds link the prebuilt DuckDB library and compile no C++. Only installer builds (`--features bundled-duckdb`, set by `build.sh`) compile DuckDB from C++, at 2–3 GB of RAM per compiler process; `build.sh` sizes its job count from free memory. Never enable `bundled-duckdb` in ad-hoc commands on a busy machine, and never run two such builds at once.

Building `parvu-app` on Linux needs `libwebkit2gtk-4.1-dev` and friends (see `docs/BUILDING.md`).

## How data flows

1. The UI calls a typed wrapper in `ui/src/lib/api.ts`, e.g. `api.applyOp(tabId, op)`.
2. `crates/parvu-app/src/commands.rs` finds the calling window's `Workspace` and runs the call on the blocking thread pool.
3. `parvu_core::Workspace` changes the tab and returns a `TabSummary`; the UI then fetches a `Page`.

### Tabs in DuckDB

Opening `sales.parquet` creates the view `__pv_src_sales` (the file) and the view `sales` (the file plus pending cell edits). The tab's `QueryEngine` holds the current SQL, starting as `SELECT * FROM "sales"`. Every query, operation and cell edit is one entry in the tab's step list; undo reverts the last entry.

- SQL typed in SQL mode uses the table name as the original file.
- Expressions run with the table name bound to the tab's current result (`Workspace::run_relative_sql`).
- Cell edits are allowed only while no transform is applied.
- **Table operations** (`Operation::is_table_op`: group by, pivot, melt, transpose, value counts, describe, join, append) don't change the tab: `Workspace::plan_result` → `workspace::materialize` (a job, into the in-memory table `__pv_res_N`, at most `RESULT_ROW_LIMIT` rows) → `add_result` opens a new tab whose source view reads that table. The tab keeps its `Origin` for Refresh until it is saved (Save As makes it a file tab).

## Conventions

### Rust
- Every identifier and literal in generated SQL goes through `sql::quote_ident` / `sql::quote_literal`; type names in `CAST` through `sql::is_safe_type_name`.
- Errors: `thiserror` enums per crate; commands return `ApiError { kind, message }`.
- New table operations are new variants of `parvu_core::ops::Operation` with a `StepLabel`; add a test in `ops.rs` and, if it touches files, in `crates/parvu-core/tests/workspace.rs`.
- Logging with `tracing`.

### UI
- All user-visible text through `t(key, args)`; add new keys to all three files in `crates/parvu-config/locales/` (en, ru, az) with the same placeholders. `cargo test -p parvu-config` checks this.
- Colours and fonts only from the theme's `--pv-*` CSS variables or the Material `--md-*` tokens derived from them at the top of `ui/src/styles.css`. Buttons: default (tonal), `primary` (filled), `text`, `outlined`, `icon`; icons through `components/Icon.tsx` (`@mdi/js`).
- Dialogs live in `ui/src/dialogs/`, one file each, opened with `openXxx(ui, args): Promise<Result>`; keep their pure logic in `dialogs/logic.ts` with tests.
- Use `PopupMenu` for menus. Never block the window's own context menu in inputs or the editor.

### Expression language
- `DROP_DUPLICATES`, `DROP_NULL` and `REPLACE` are handled in `resolver.rs` before the function registry; registering them in `registry.rs` only enables autocomplete.
- `tests/golden/dsl.json` pins compiled SQL for 306 expressions. If you change compiler output on purpose, update the expected SQL there.

## When you need to change…

| Task | Where |
|------|-------|
| Column types: detection on open, Read as, conversions | `parvu-core/src/types.rs` (`Reading`, detectors, `best_reading`), `ops.rs` (`Convert`, `ConversionErrors`, `ColumnTransform`, `MergeColumns`, `DateDiff`, `DateRange`, `ExpandJson`, `ExpandStruct`, `ToRows`), `colfn.rs` (text, yes/no, date and JSON/list functions, relative date ranges, `JsonKey`), `types.rs` `CsvOptions` (Open with options, remembered in the app's `column_types.rs`), `workspace.rs` (`heal`, `set_source_reading`, `preview_conversion`); UI `lib/types.ts`, `lib/colfn.ts`, `dialogs/ConvertDialog.tsx`, `ColumnFnDialog.tsx`, `MergeColumnsDialog.tsx`, `DatesDialog.tsx`, `ExpandJsonDialog.tsx`, `OpenOptionsDialog.tsx`; plan `plans/COLUMN_TYPES_PLAN.md` |
| A table operation | `parvu-core/src/ops.rs` (or `reshape.rs`, `clean.rs`, `window.rs`), the menu or dialog in `ui/src/App.tsx` / `ui/src/dialogs/` |
| Charts | `parvu-core/src/chart.rs`, `ui/src/components/Chart.tsx`, `ui/src/dialogs/ChartDialog.tsx` |
| Conditional formatting | `ui/src/lib/format.ts`, `ui/src/dialogs/ConditionalFormatDialog.tsx`, applied in `DataGrid.tsx` |
| A new backend command | `parvu-app/src/commands.rs`, register in `main.rs`, wrapper in `ui/src/lib/api.ts` |
| Work that can scan a whole table (stats, unique values, charts, search, counts) | run it as a job: `parvu-app/src/jobs.rs` (`run_job`: cloned connection, no workspace lock, progress, cancel), `parvu-core/src/job.rs` (`Cancel`); UI `lib/jobs.ts` (`startJob`), `components/JobProgress.tsx`. Never hold `with_ws` for a full scan, and stream results without a LIMIT (`stream_arrow`) |
| Query, paging or undo behaviour | `parvu-core/src/engine.rs`, `workspace.rs` |
| Cell edits or saving | `parvu-core/src/edits.rs`, `workspace.rs` |
| File format support | `parvu-core/src/format.rs` (and `export.rs` for writing) |
| Export options | `parvu-core/src/export.rs`, `ui/src/dialogs/ExportDialog.tsx` |
| An expression function | `parvu-dsl/src/registry.rs`; resolver/compiler for special forms |
| A setting | `parvu-config/src/settings.rs`, `ui/src/dialogs/SettingsDialog.tsx`, locale files |
| A theme colour | `parvu-config/src/themes.rs` (emits `--pv-*`), then use the variable in CSS |
| A translation or language | `crates/parvu-config/locales/`, see `docs/I18N.md` |
| Keyboard shortcuts | the `onKey` handler in `ui/src/App.tsx`, `dialogs/ShortcutsDialog.tsx` |
| Packaging | `crates/parvu-app/tauri*.conf.json`, `crates/parvu-app/packaging/`, `build.sh` |
| CI | `.github/workflows/ci.yml` (tests), `package.yml` (installers) |
| MCP server (`parvu mcp`) | `parvu-app/src/mcp.rs` (tools, `--allow-dir` / `--allow-write`, the `--attach` bridge and TCP listener), user guide `docs/MCP.md`; tests with `cargo test -p parvu-app` |
| Built-in AI assistant | model side: `parvu-ai` (`provider/anthropic.rs`, `provider/openai.rs`, `agent.rs`); app side: `parvu-app/src/ai_assistant.rs` (keys, providers.json, tools, runs); UI: `components/AiPanel.tsx`, `dialogs/AiProvidersDialog.tsx`, `lib/ai.ts`; user guide `docs/AI.md`. Actions on the assistant's SQL and results (copy, open in a new tab, edit in SQL editor): `AiPanel.tsx` (`AiActions`, wired in `App.tsx`), logic in `lib/ai.ts` (`toolResultTable`, `toolResultSql`, `markdownSegments`, `sqlEditBlocker`), `lib/sqlHighlight.ts`; a new tab runs `Derivation::TabsSql` (`Workspace::tabs_sql`: table names = tabs' current results, through the read-only guard), tests in `parvu-core/tests/readonly.rs` |
| AI clients in the window (attached mode) | `parvu-app/src/mcp_attach.rs` (listener, approval cards and attention requests, `mcp_*` commands), `bridge.rs` (`parvu mcp --attach`: offline answers, starting ParVu on the first tool call, launch lock), `instance.rs` (one app per profile, `--background`, idle quit; `PARVU_BACKGROUND_IDLE_SECS`), `ui/src/dialogs/ConnectAiDialog.tsx`, `McpProposalDialog.tsx`, the AI menu and listeners in `App.tsx` |
| Tray icon, closing to the tray, `--quit` | `parvu-app/src/tray.rs` (icon and menu, `close_action`, `quit_plan`, the `window_close_action` / `hide_to_tray` commands, the Linux tray-host check), `instance.rs` (`--background` starts hidden, `--quit`, `untouched_after` for the idle quit), `main.rs` (`build_background_window`, `handle_second_start`); setting `keep_running_in_tray` in `parvu-config/src/settings.rs` + `SettingsDialog.tsx`; the close flow and `quit-requested` in `App.tsx`, `windowIsDirty` in `dialogs/logic.ts`; guide `docs/MCP.md` (The tray icon). Linux loads `libayatana-appindicator3` at run time; only bundling the deb needs `-dev` |
| Read-only access for AI / MCP (SQL guard, previews) | `parvu-core/src/readonly.rs`, `Workspace::read_session` / `preview_op`, tests in `parvu-core/tests/readonly.rs`; see `plans/AI_PLAN.md` §6 |
| Custom functions and operations | core: `parvu-core/src/custom/` (`signature.rs`, `compile.rs` body → template, `validate.rs` checks on its own DuckDB, `examples.rs`, `fill.rs` arguments and the SQL an application builds, `sqlmode.rs` custom calls in SQL mode and MCP `query` expanded by rewriting the parsed query), `record.rs` (steps → operation, from the `Recipe` each query step keeps in `workspace.rs`, `Workspace::steps_capture`), `Operation::CustomFunction` / `CustomOperation` in `ops.rs`, custom aggregates in `reshape.rs` (`AggFunc::Custom`, `is_custom_aggregate`), `Workspace::set_custom`, formula totals off the lock (`formula_totals_job`, `set_resolved_formulas`); samples in `parvu-core/samples/*.json`; app: `custom_store.rs` (`~/.ParVu/custom`, seeding, revalidation, import/export), `custom.rs` (commands; `registry` / `registry_for`: the expression language's functions plus the window's custom functions, `function_def`, `expression_body_sql`, `refresh_formulas`, `custom_from_steps`, MCP listing and proposals); expression language: `FunctionDef::custom` (`arg_slots`, `aggregate`, `window`), `ParamDef::accepts` in `parvu-dsl/src/registry.rs`, type check in `resolver.rs`; MCP: `list_custom_tools`, `propose_custom_tool` in `mcp.rs` (`AttachHost::offer_custom`, `custom-proposal` event); UI: `dialogs/CustomToolsDialog.tsx`, `CustomEditorDialog.tsx`, `CustomApplyDialog.tsx`, `SaveStepsDialog.tsx`, custom aggregates in `GroupByDialog.tsx`, logic in `dialogs/logic.ts`, completions in `lib/exprComplete.ts` and `lib/sqlComplete.ts`; tests `parvu-core/tests/custom.rs`, `custom_sql.rs`, golden cases in `parvu-dsl/tests/golden.rs`; guide `docs/CUSTOM.md`, plan `plans/CUSTOM_TOOLS_PLAN.md` |
| Releases | `.github/workflows/release.yml`, triggered by a `v*` tag; steps in `RELEASES.md` |

## Known pitfalls

1. **Compiling DuckDB** (`bundled-duckdb`) can exhaust memory; it is only for installers (see above).
   **Whole-table aggregates** (`MODE`, `COUNT(DISTINCT …)`, or anything with no GROUP BY over a big table) can grow past DuckDB's memory limit: build them with `sql::whole_table*`. Probe heavy queries only inside `systemd-run --user --scope -p MemoryMax=… -p MemorySwapMax=0`. The memory guard (`parvu-app/src/memguard.rs`) is the last line of defence, not a substitute.
2. **Grid coordinates:** Glide Data Grid rows are page-relative; add `page.offset` for absolute rows (edits, search).
3. **React StrictMode** runs effects twice in development; one-time startup work is guarded (`bootstrapOnce` in `main.tsx`, `started` ref in `App.tsx`).
4. **Tauri build hooks** (`beforeDevCommand` / `beforeBuildCommand`) run from the `crates/` folder, not from `crates/parvu-app`.
5. **Long queries and the workspace lock:** `with_ws` holds the window's workspace for the whole call, so a full-table scan there freezes paging and menus. Use `run_job` for such work; sync (non-async) commands must never lock a workspace, because they run on the main thread.
6. **Debian versions:** dpkg would rank `1.0.0-alpha.1` above `1.0.0`, so `build.sh` repacks pre-release debs as `1.0.0~alpha.1` (`crates/parvu-app/packaging/fix-deb-version.sh`). Keep that step if you change how the deb is built.
