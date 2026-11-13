# ParVu Rust Rewrite — Plan

> Status: phases 0–6 implemented on the working tree, Phase 7 (beta and cutover) not started. See [§12](#12-implementation-status).
> Target: ParVu 1.0 as a Rust application, replacing the Python/PyQt6 app.
> Owner: @AzizNadirov.
> Last revised: 2026-11-12.

This plan replaces the Python/PyQt6 application with a Rust application that keeps the same features, file formats and user data. It is written for an engineer who knows the current codebase. Decisions that must be made before work starts are in [§2](#2-decisions-to-make-first). Open questions are in [§11](#11-open-questions).

---

## 1. Why, and what we expect to gain

DuckDB already does the heavy query work in C++, so the rewrite will **not** make queries faster. The gains are elsewhere:

| Problem today | Cause | Expected after rewrite |
|---|---|---|
| Installed deb is 483 MB | Python, pandas, pyarrow, numpy and Qt are all bundled | 40–70 MB, most of it DuckDB |
| Slow cold start | Interpreter start plus pandas/pyarrow imports | Native start, well under 1 s |
| Fragile packaging | PyInstaller data layout; the 0.4.0 deb crashed on first run because resources landed in the wrong folder | One binary, defaults compiled in with `include_str!` |
| Page display cost | DuckDB → pandas DataFrame → one `QTableWidgetItem` per cell | DuckDB → Arrow batch → virtual grid, no per-cell objects |
| Cell-edit save loads the whole file into pandas | `edit_queue.read_source_file` / `write_source_file` | DuckDB applies edits and writes the file with `COPY` |

Phase 0 measures the current numbers so these claims are checked, not assumed.

### Non-goals
- New user-facing features during the rewrite. The rewrite reaches parity first; features come after cutover.
- Keeping the Python plugin API. `src/parvu/plugins/` has the base classes and registry but no shipped plugins, so nothing breaks for users.
- Faster queries. Query speed stays DuckDB-bound.

---

## 2. Decisions to make first

### 2.1 GUI toolkit (decided by the Phase 0 spike)

Qt bindings for Rust (`cxx-qt`) need QML and still ship Qt, which throws away the size gain. The real candidates are:

| | **Tauri 2 + TypeScript UI** (recommended) | **egui** (pure Rust) | **Slint** |
|---|---|---|---|
| Big data grid | Glide Data Grid: canvas, virtualised, millions of rows | `egui_extras::TableBuilder`, virtualised, fast | `StandardTableView`, weak for wide/huge tables |
| SQL editor with highlight + autocomplete | CodeMirror 6 with `@codemirror/lang-sql`, solved | Custom `TextEdit` layouter; autocomplete is hand-built | Hand-built |
| Theming | CSS variables, close to today's QSS model | `egui::Visuals`, limited | Slint styles |
| AI chat panel ([AI plan](AI_PLAN.md) §2) | Markdown, streaming, cards are easy | Hand-built | Hand-built |
| Languages | Rust + TypeScript | Rust only | Rust + `.slint` |
| Linux dependency | `libwebkit2gtk-4.1` (in Ubuntu 22.04+) | None | None |
| Licence | MIT/Apache | MIT/Apache | GPLv3 or royalty-free with attribution |

**Recommendation:** Tauri 2, with Svelte (or Solid) for the UI, CodeMirror 6 for both editors and Glide Data Grid for the table. The editor, grid and chat panel are the hardest parts of ParVu's UI, and the web ecosystem already has mature versions of all three. If "Rust only" is a hard requirement, pick egui and budget an extra 3–4 weeks for the editors.

### 2.2 Order relative to the AI milestone

[AI_PLAN.md](AI_PLAN.md) builds on Pydantic AI, which has no Rust equivalent. Building AI in Python and then porting it doubles the work.

**Recommendation:** freeze Python features after the export-dialog branch merges, do the rewrite, then build AI on the Rust app. Keep the AI plan's **capability layer** (§3 there: one definition drives the menu action, the DSL function and the AI tool). It is language-agnostic, and this plan builds it into the Rust core from day one (§5.3). Provider calls move to `reqwest` directly or a crate such as `genai`; credentials use the `keyring` crate.

### 2.3 Feature freeze

1. Merge `feat/export-dialog` (version 0.4.0) to `main`.
2. Tag `v0.4.x` as the last Python release line. Only bug fixes land there.
3. Every Python fix after the freeze also gets a ticket on the Rust side, so parity does not drift.

---

## 3. Target architecture

The current layering (Presentation → Services → Infrastructure/Config → Core) maps onto a Cargo workspace. Rust's crate boundaries enforce what `AGENTS.md` today enforces by convention.

```
Cargo.toml                      # workspace
crates/
├── parvu-core/                 # replaces parvu.core (minus dsl)
│   ├── engine.rs               # QueryEngine: duckdb connection, history/undo, paging
│   ├── adapters.rs             # Parquet / CSV / JSON / Excel readers
│   ├── pagination.rs
│   ├── edits.rs                # EditQueue + write-back via DuckDB
│   ├── export.rs               # ports the export-dialog exporter
│   ├── stats.rs                # column stats, unique values, samples
│   ├── search.rs               # find_next_match
│   └── capabilities/           # one definition per operation (see §5.3)
├── parvu-dsl/                  # replaces parvu.core.dsl
│   ├── grammar.pest
│   ├── parser.rs  resolver.rs  ir.rs  compiler.rs
│   └── registry.rs  types.rs  catalog.rs
├── parvu-config/               # replaces parvu.config + i18n + theme models
│   ├── settings.rs  recents.rs  app_state.rs
│   ├── i18n.rs                 # loads locales/*.toml
│   └── themes.rs               # Theme model → CSS variables
└── parvu-app/                  # Tauri backend: commands, tab state, workers
ui/                             # TypeScript frontend (Svelte)
├── grid/  editor/  dialogs/  panels/
locales/en.toml  ru.toml  az.toml
```

### Crate choices

| Need | Python today | Rust |
|---|---|---|
| DuckDB | `duckdb` | `duckdb` crate with `bundled` and `arrow` features |
| Results to UI | pandas DataFrame | Arrow `RecordBatch`, sent to the UI as Arrow IPC bytes (`tauri::ipc::Response`), read with `apache-arrow` JS |
| DSL grammar | Lark | `pest` (grammar is small and maps 1:1, see `core/dsl/parser.py`) |
| SQL generation | sqlglot AST → `.sql(dialect="duckdb")` | `sqlparser` AST built by hand, printed with `Display`; DuckDB-only, so no transpiling is needed |
| Excel read | DuckDB `read_xlsx` (needs the excel extension, downloaded at runtime) | `calamine` → Arrow → DuckDB appender; works offline |
| Excel write | pandas `to_excel` | `rust_xlsxwriter` |
| Settings | Pydantic | `serde` + `serde_json` |
| Logging | loguru | `tracing` + `tracing-appender` to `~/.ParVu/logs` |
| Crash reports | `crash_reporter.py` | `std::panic::set_hook` writes a report, UI shows the dialog on next start |
| Background work | `QThread` workers in `presentation/workers.py` | Tokio tasks; DuckDB calls on `spawn_blocking`, one connection per tab |

---

## 4. Migration strategy

**Rewrite alongside, compare against the Python app, cut over once.** The Python app stays the shipping product until the Rust app passes the parity checklist (§7).

The Python code is used as a test oracle:
- **DSL golden files.** A script runs the Python parser, resolver and compiler over a corpus of expressions and writes `{expression → SQL or error}` to `tests/golden/dsl.json`. Rust tests must produce the same SQL, or SQL that returns the same rows on the sample files.
- **Engine golden files.** The same idea for `QueryEngine`: row counts, column types, page contents, stats and search results on a fixed set of sample Parquet, CSV, JSON and Excel files.

**Considered and rejected:** porting the core to Rust first and loading it into the PyQt app through PyO3. It gives early validation, but it means building and packaging a PyO3 module inside PyInstaller, and the golden files give the same validation for less work.

---

## 5. Phases

Estimates are for one developer working full time. Part-time work stretches them proportionally.

### Phase 0 — Baseline and spike (1–2 weeks)
- Measure the Python app on a fixed machine: deb size, cold start to first window, time to show page 1 of a 1 GB Parquet file, page-switch time at 1,000 rows × 50 columns, memory after load. Record in `plans/rust_baseline.md`.
- Spike the toolkit (§2.1). A throwaway app that opens a Parquet file with `duckdb-rs`, shows 10 million rows in the grid with smooth scrolling, and has a SQL editor with column-name autocomplete. Build it with Tauri, and with egui if Rust-only is wanted.
- Check `duckdb` `bundled` build time and binary size on Linux and Windows.
- **Exit:** toolkit chosen, baseline recorded.

### Phase 1 — Core crate (3–4 weeks)
- `QueryEngine` with the same contract as `core/query_engine.py`: base query, table-name substitution, `apply_transform` with history, `undo`, `reset_query`, paging, `get_columns`, `get_column_types`, `get_table_info`, `sort_by_column`, `export_results`.
- File adapters, including Excel through `calamine`.
- `Paginator`, column stats, unique values, samples, `find_next_match`.
- **Edit write-back without pandas.** Load the file into a DuckDB table with an explicit row index, apply `UPDATE … SET col = CAST(? AS <type>)` per edit, `COPY` to a temp file in the same format, then rename over the original. DuckDB casting replaces the pandas dtype guessing in `apply_edits_to_dataframe`.
- Port the exporter from `feat/export-dialog` (type casting, date formats, progress callback).
- **Exit:** engine golden files pass. Unit tests cover what `tests/unit/core/` covers today.

### Phase 2 — DSL crate (2–3 weeks)
- Port the grammar to `pest` and keep the operator set and precedence exactly.
- Port the resolver, including the special case for `DROP_DUPLICATES` before registry lookup.
- Compiler emits the same SQL shapes: assignment as `SELECT *, expr AS "col" FROM data`, drop-duplicates with `QUALIFY ROW_NUMBER() OVER (PARTITION BY …)`.
- Port `FunctionRegistry` with its docs, since the editor autocomplete and doc popup read from it.
- **Exit:** DSL golden files pass. Everything in `tests/unit/core/dsl/` is ported.

### Phase 3 — Config, i18n, themes (1 week)
- `Settings` reads the **existing** `~/.ParVu/settings/settings.json` unchanged. Several fields are `int | str` today and stored as strings such as `"500"`, so deserialisation must accept both.
- Recents and app state read their existing files too.
- A one-off script exports the three Python locale dicts (about 370 keys each) to `locales/*.toml`, keeping key names and `{placeholder}` syntax. A test fails if any locale is missing a key.
- Theme models move to `serde`. The QSS generator in `infrastructure/themes/stylesheet.py` becomes a CSS-variable generator. Built-in themes keep their names so `current_theme` still resolves.
- **Exit:** a user's existing `~/.ParVu` folder loads with no migration step.

### Phase 4 — App shell (3–4 weeks)
- Main window: file open (dialog, CLI argument, drag and drop), tabs with one engine per tab, file toolbar visibility rules, pagination bar.
- Data grid fed by Arrow pages: header sort, header and cell context menus, copy as CSV/TSV/Markdown, Shift+wheel horizontal scrolling, edited-cell highlighting.
- Query editor with SQL and Expression modes, highlighting, autocomplete from the catalog and function registry, function doc popup.
- Applied-steps panel with undo. Every step-appending action pushes to both the steps list and the undo stack, as `AGENTS.md` requires today. Build this as one function so it cannot be forgotten.
- **Exit:** open a file, query it in both modes, page through it, undo steps.

### Phase 5 — Feature parity (6–8 weeks)
Work through the checklist in §7. Each operation is defined once as a capability (§5.3), then wired to the operations panel, the menu, and where relevant the DSL.

### Phase 6 — Packaging and CI (1–2 weeks)
- Linux: Tauri bundler or `cargo-deb`. Carry over the desktop file, MIME definitions for Parquet, icon sizes, and the `postinst`/`postrm` database updates from `build.sh`. Install under `/usr/bin`, not `/usr/local/bin`.
- Windows: MSI or NSIS from the Tauri bundler, replacing `build.ps1`.
- CI on GitHub Actions: `cargo test`, `cargo clippy -D warnings`, UI tests, golden-file tests, and build artefacts for both platforms.
- A smoke test in CI launches the built package with an empty home directory, which is the case that broke the 0.4.0 deb.

### Phase 7 — Beta and cutover (2–4 weeks)
- Ship as a separate `parvu-next` package that can be installed next to the Python app and uses the same `~/.ParVu`.
- Compare against the Phase 0 baseline and the parity checklist.
- Cut over: the `parvu` package becomes the Rust app at 1.0. Move the Python source to a `legacy/` branch and delete it from `main`.

### Total
About **20–28 weeks** of full-time work. The UI phases (4 and 5) are more than half of it, which matches the code: the Qt presentation layer is about 7,000 of the 12,900 Python lines.

### 5.3 Capability layer in the core
Define each operation once, in `parvu-core/capabilities/`:

```rust
pub struct Capability {
    pub id: &'static str,            // "drop_duplicates"
    pub label_key: &'static str,     // i18n key for menus and dialogs
    pub params: &'static [ParamDef], // drives dialog forms and DSL argument checks
    pub mutates: bool,               // AI approval flow needs this later
    pub build_sql: fn(&Table, &Args) -> Result<String, CapError>,
}
```

Dialogs that only collect parameters (drop duplicates, drop null, replace, join, append) can then render from `params`, which removes a lot of hand-written dialog code. The AI milestone later turns the same registry into tools.

---

## 6. Testing

| Layer | How |
|---|---|
| Core and DSL | `cargo test`, plus the golden files from §4 |
| Config | Load fixtures copied from real `~/.ParVu` folders, including old settings files |
| UI | Component tests with Vitest; end-to-end flows with Tauri's WebDriver support |
| Packaging | CI smoke test with an empty home directory |
| Performance | A benchmark job repeats the Phase 0 measurements and fails on regressions over 20% |

Port tests as each module is ported, so the Rust suite covers at least what the current 119 Python tests cover before cutover.

---

## 7. Parity checklist

Tick each item only when it works in the Rust app **and** matches the Python behaviour.

**Data and query**
- [ ] Open Parquet, CSV, JSON, Excel (CLI, dialog, drag and drop, recent files)
- [ ] Lazy loading of very large files (8 GB+), large-dataset warning by rows, cells or file size
- [ ] SQL mode, Expression mode, mode switching
- [ ] Pagination, rows-per-page setting, max rows
- [ ] Sort by header, applied steps, undo, reset
- [ ] Cell editing, edit highlighting, save back to source, unsaved-changes and exit warnings
- [ ] Find bar and search dialog
- [ ] Export with type casting, date formats, progress

**Operations and dialogs**
- [ ] Drop duplicates, drop null, replace values, unique-values filter
- [ ] Rename, remove, duplicate, change type of a column
- [ ] Join, append
- [ ] Column statistics, table info, copy column as tuple
- [ ] Settings, theme selector, language selector
- [ ] Expression help, shortcuts, about, crash reporter

**App**
- [ ] Multiple tabs with independent state
- [ ] Themes: all built-in themes, custom themes from `~/.ParVu/themes`
- [ ] Languages: English, Russian, Azerbaijani, with no missing keys
- [ ] Existing `~/.ParVu` settings, recents and themes load unchanged
- [ ] Keyboard shortcuts match the shortcuts dialog
- [ ] Linux deb and Windows installer, file associations

---

## 8. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Rewrite drags on while Python keeps getting features | Parity never reached | Feature freeze (§2.3); fixes mirrored as tickets |
| Grid or editor quality worse than Qt's | Users notice a regression | Phase 0 spike tests exactly these two before committing |
| DSL behaviour drifts | Saved expressions give different results | Golden files from the Python implementation |
| WebKitGTK quirks on older Linux distributions | Rendering bugs | Support Ubuntu 22.04+ and equivalents; test on the oldest supported distribution in CI |
| DuckDB `bundled` builds are slow (C++ compile) | Slow CI | Cache `target/`; build DuckDB once per version |
| Settings files written by older versions fail to parse | Crash on first start | Lenient deserialisation, fall back to defaults with a warning; fixture tests (Phase 3) |
| Single maintainer | Long calendar time | Phases ship usable pieces; Phase 7 beta can run for as long as needed |

---

## 9. Success criteria

Checked against the Phase 0 baseline before cutover.

| Metric | Target |
|---|---|
| Linux package size | at most 70 MB |
| Cold start to usable window | under 1 s |
| Page switch, 1,000 rows × 50 columns | under 100 ms |
| Memory after opening a 1 GB Parquet file | at most the Python app's |
| Parity checklist | 100% |
| First-run crash on clean home directory | none, tested in CI |

---

## 10. What changes for users

- Smaller download and faster start.
- Same settings, recents, themes and languages, with no migration step.
- Excel files open without an internet connection.
- The Python plugin API goes away. No plugins ship today.

---

## 11. Open questions

1. **Toolkit:** Tauri or egui? Decided by the Phase 0 spike, unless "Rust only" is a requirement.
2. **AI timing:** confirm the AI milestone moves after the rewrite (§2.2).
3. **Platforms:** is macOS in scope for 1.0? It is not supported today.
4. **Settings location:** keep `~/.ParVu`, or move to XDG folders (`~/.config/ParVu`, `~/.local/share/ParVu`) at 1.0 with a one-time move? `infrastructure/paths.py` already has the XDG helpers.
5. **Plugins:** drop for good, or plan a WASM-based plugin API after 1.0?

---

## 12. Implementation status

Last updated 2026-11-13.

### Measured so far

| | Python 0.4.0 | Rust beta |
|---|---|---|
| Package download | not measured | 30 MB `.deb` |
| Installed size | 483 MB | 82 MB |
| Memory with a 100,000-row Parquet file open | not measured | about 230 MB |
| Launch to file opened (typical) | 620–760 ms | about 400 ms |
| Page switch, 1,000 rows × 10 columns | about 190 ms | about 8 ms |

Start-up was timed on a virtual X display from process launch until the log records the file as opened. Rust runs were usually about 400 ms, with occasional runs near 900 ms. Page switch for Python covers fetching the page and filling the Qt table; for Rust it covers fetching and formatting the page, but not sending it to the UI or drawing it. The page benchmark is `cargo run --release -p parvu-core --example page_bench -- FILE`.

The Rust beta was checked end to end on a headless X display: opening files from the command line, SQL and expression queries, sorting, filtering, joins, Excel export, cell edits and save, search across pages, language and theme switching, and the unsaved-changes guard.

### Done

| Phase | State |
|---|---|
| 0. Spike | Toolkit chosen: Tauri 2. The UI uses **React** instead of Svelte, because Glide Data Grid is a React component. Baseline numbers are not recorded yet. |
| 1. Core | `crates/parvu-core`: engine, Parquet/CSV/JSON/Excel sources, paging, cell edits, operations, export, statistics, search. |
| 2. DSL | `crates/parvu-dsl`: pest grammar, resolver, compiler, registry, golden tests against the Python implementation (`scripts/dsl_golden.py`). |
| 3. Config | `crates/parvu-config`: settings, recents, app state, translations, themes. Reads existing `~/.ParVu` folders unchanged. |
| 4–5. App and UI | `crates/parvu-app` (Tauri commands) and `ui/` (React): every window feature and dialog from the parity checklist in §7. |
| 6. Packaging | `build-rust.sh` builds a `parvu-next` beta package by default and the final `parvu` package with `--final`. CI (`.github/workflows/rust.yml`) passes on Linux and Windows: tests and clippy with a prebuilt DuckDB (about 4 minutes), Python tests, and on `main`, tags or manual runs the `.deb` (28 MB) and Windows `setup.exe` (14 MB) installers plus a clean-home smoke test on Linux. Windows ships an NSIS installer only, because MSI rejects pre-release versions. |

### Changes from the plan

- **Pages are sent as formatted text, not Arrow IPC.** Rust formats each cell with Arrow's own formatter. The UI needs no type-specific formatting code, and a page of a few hundred rows is small.
- **Translations moved to JSON.** `crates/parvu-config/locales/*.json` is now the source of truth. Keys were exported once from the Python locale files with `scripts/export_locales.py`, and new keys were added there in all three languages. Running the export script again would drop the new keys.
- **Cell edits use an overlay view** instead of rewriting the file: each tab's view replaces edited cells by row number, so queries, joins and saves see edits without loading the file into memory.

### Python behaviour that was fixed rather than copied

- Join, append and the value filter read the original file and dropped earlier steps. They now use each tab's current result.
- "Keep last" in drop duplicates used `rowid`, which does not exist on a view.
- Some transforms inserted column names without quoting them.
- The steps panel and the engine history could get out of step after SQL-mode queries. Every query is now a step, kept in one list.
- The DSL dropped parentheses, compiled `FALSE` to `TRUE`, and crashed on unary operators.
- Reset filled the editor with `FROM data` even when the table had another name.
- An expression assigning to another tab replaced the active tab's data. It is now refused.

### Remaining work

1. Run the Windows installer on a real Windows machine. CI builds it and runs the tests on Windows, but nobody has opened the app there yet.
2. Phase 7: ship `parvu-next` to users, collect feedback, then cut over with `./build-rust.sh --final` and remove the Python code.
3. Decide the open questions in §11. Plugins were dropped; settings stay in `~/.ParVu`.
