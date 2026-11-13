# ParVu — Agent Guide

How to find your way around ParVu and change it safely.

## Project at a glance

**ParVu** is a desktop app for viewing and querying Parquet, CSV, JSON and Excel files.

- **Backend:** Rust (edition 2024), DuckDB through the `duckdb` crate, in a Cargo workspace
- **Window:** Tauri 2 (system webview)
- **UI:** React 18 + TypeScript + Vite in `ui/`; Glide Data Grid for the table; CodeMirror 6 for the editors
- **User data:** `~/.ParVu` (settings, recent files, themes, logs), same format as the old Python app
- **Entry point:** `crates/parvu-app/src/main.rs`

The app was rewritten from Python/PyQt6; the plan and its status are in `plans/RUST_REWRITE_PLAN.md`. `plans/AI_PLAN.md` was written for the Python app and needs revising before use.

## Layout

```
crates/parvu-core     DuckDB engine: tabs, paging, edits, operations, export, stats, search
crates/parvu-dsl      expression language: parser → resolver → compiler (no DuckDB)
crates/parvu-config   settings, recents, themes, translations (no DuckDB)
crates/parvu-app      Tauri app: commands.rs, state.rs, main.rs, packaging
ui/src                React UI: App.tsx, components/, dialogs/, lib/
docs/                 user and developer guides
```

Dependencies only point down: `ui → parvu-app → (parvu-core, parvu-dsl, parvu-config)`. Keep DuckDB out of `parvu-dsl` and `parvu-config`, and Tauri out of all three library crates. More detail in `docs/ARCHITECTURE.md`.

## Commands

```bash
cargo test -p parvu-core -p parvu-dsl -p parvu-config      # Rust tests (prebuilt DuckDB, seconds)
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

## Conventions

### Rust
- Every identifier and literal in generated SQL goes through `sql::quote_ident` / `sql::quote_literal`; type names in `CAST` through `sql::is_safe_type_name`.
- Errors: `thiserror` enums per crate; commands return `ApiError { kind, message }`.
- New table operations are new variants of `parvu_core::ops::Operation` with a `StepLabel`; add a test in `ops.rs` and, if it touches files, in `crates/parvu-core/tests/workspace.rs`.
- Logging with `tracing`.

### UI
- All user-visible text through `t(key, args)`; add new keys to all three files in `crates/parvu-config/locales/` (en, ru, az) with the same placeholders. `cargo test -p parvu-config` checks this.
- Colours and fonts only from the theme's `--pv-*` CSS variables.
- Dialogs live in `ui/src/dialogs/`, one file each, opened with `openXxx(ui, args): Promise<Result>`; keep their pure logic in `dialogs/logic.ts` with tests.
- Use `PopupMenu` for menus. Never block the window's own context menu in inputs or the editor.

### Expression language
- `DROP_DUPLICATES`, `DROP_NULL` and `REPLACE` are handled in `resolver.rs` before the function registry; registering them in `registry.rs` only enables autocomplete.
- `tests/golden/dsl.json` pins compiled SQL for 306 expressions. If you change compiler output on purpose, update the expected SQL there.

## When you need to change…

| Task | Where |
|------|-------|
| A table operation | `parvu-core/src/ops.rs`, the menu or dialog in `ui/src/App.tsx` / `ui/src/dialogs/` |
| A new backend command | `parvu-app/src/commands.rs`, register in `main.rs`, wrapper in `ui/src/lib/api.ts` |
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
| CI | `.github/workflows/ci.yml` |

## Known pitfalls

1. **Compiling DuckDB** (`bundled-duckdb`) can exhaust memory; it is only for installers (see above).
2. **Grid coordinates:** Glide Data Grid rows are page-relative; add `page.offset` for absolute rows (edits, search).
3. **React StrictMode** runs effects twice in development; one-time startup work is guarded (`bootstrapOnce` in `main.tsx`, `started` ref in `App.tsx`).
4. **Tauri build hooks** (`beforeDevCommand` / `beforeBuildCommand`) run from the `crates/` folder, not from `crates/parvu-app`.
5. **Debian versions:** `1.0.0-alpha.1` sorts after `1.0.0` in dpkg; before the first stable release, check that the stable package upgrades the alpha.
