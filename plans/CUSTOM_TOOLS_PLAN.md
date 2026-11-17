# Custom functions and custom operations — design

Status: phases 1–7 implemented (see "Implementation notes" below), except SQL-mode macros. File paths are relative to the repo root. User guide: `docs/CUSTOM.md`.

**What was not verified when this was written.** No DuckDB was available on this machine (no `duckdb` Python package, no CLI), so every statement about DuckDB behaviour is from memory. Those statements are marked **[verify]** and listed in §11.4. The repo facts were read from the code.

### Implementation notes (phases 1–4)

Decisions taken for this round: bodies are SQL only (a SQL expression for functions, one SELECT for operations; `BodyLanguage` is the seam for expression bodies); at least one passing example is required to Save, Save draft keeps the rest; applied steps keep their SQL and Refresh uses the current definition; no AI or MCP defining; no SQL-mode macros; no "name" parameter kind.

§11.4 checked against the shipped DuckDB (v2.0.0, prebuilt library):

1. `json_serialize_sql` / `json_deserialize_sql` round-trip `QUALIFY`, windows with frames, `* REPLACE`, `* EXCLUDE`, CTEs, lambdas, `PIVOT`, `ASOF JOIN`, `UNION ALL BY NAME`. Non-SELECT statements are refused ("Only SELECT statements can be serialized"). `GROUP BY ALL ORDER BY ALL` comes back as `ORDER BY COLUMNS(*)` (same meaning). `SELECT 1; SELECT 2` serializes as two statements, which the guard refuses. True, with these notes.
2. `duckdb_functions()` has `stability` (`CONSISTENT`, `CONSISTENT_WITHIN_QUERY`, `VOLATILE`) and `has_side_effects`. True; the fixed fallback list is not needed. **Deviation:** `current_date`, `current_timestamp` and friends written without parentheses parse as column references, not functions, so the compiler also treats those names as reading the clock.
3. A fresh `Connection::open_in_memory()` has the JSON functions. True; validation uses its own database as designed (256 MB, 2 threads, external access off).
4. `DESCRIBE` over a `WHERE false` relation gives the same types as over data. True (checked for sums and divisions of DECIMAL and BIGINT).
5. A non-grouped column beside an aggregate fails with `Binder Error: column "b" must appear in the GROUP BY clause …`. The shape probe matches "Binder Error" and "GROUP BY".

Other deviations:

- `columns` (several columns) parameters are not implemented (they are phase 7 "everywhere"); signatures have column, value and table parameters.
- The cache hash is FNV-1a 64 of the source parts, not SHA-256: it is only a cache key, and `app`/`duckdb` versions are checked too. No new dependency.
- A row function written to a new column puts it right after its first column argument (as Column → Text functions do), not at the end.
- Examples store `input` (a function's columns, or an operation's first table), `tables` (other tables), `args` and `expected` as small tables (see `docs/CUSTOM.md`) rather than §4.5's per-kind shapes. Mismatches are `[row, column]`, `[row, -1]` (an expected row not found), `[-1, -1]` (column names), `[-2, -1]` (extra rows).
- Dependencies between custom tools (§6.6) don't arise yet: a SQL body can't call a custom function. Deleting is therefore never refused.
- The name check also refuses DuckDB function names (`weighted_avg` is one, so the sample is `weighted_average`).
- A headless `parvu mcp` loads the user's usable definitions, so `apply_operation` can run custom calls; `list_custom_tools` and proposing from AI came in phase 6.

### Implementation notes (phases 5–6)

Phase 5, expression language and formulas:

- `parvu-dsl` stays free of DuckDB. `FunctionDef` gained `arg_slots` (the template marks argument `i` as `__pv_arg_i`; `compiler::fill_slots` fills them in one pass, so `{2}` in a regex is not a slot), `aggregate`, `window` and `custom`; `ParamDef` gained `accepts: Option<Vec<LogicalType>>`. `FunctionDef::custom` builds one.
- The resolver refuses an argument whose logical type is known and not accepted ("clean_phone: phone takes text, but this argument is a number"), and extra arguments to a custom function. `Unknown` (an arithmetic result, NULL, JSON, HUGEINT…) is let through: DuckDB binds it. A column type set with a family the expression language can't name (`any`, `json`, `time`…) has no `accepts`.
- `compile_formula` treats `aggregate` functions as totals and `window` functions as reading neighbours (`reads_neighbours` now looks at the registry).
- App side (`custom.rs`): `function_def` turns a usable definition into a `FunctionDef`. **Deviation:** value arguments are cast to the declared type in the template the DSL fills (`CAST(__pv_arg_i AS dtype)`), and an optional value's default is `value_literal` of it (`CAST(NULL AS dtype)` when there is none). Choices and min/max of value parameters are checked only in the apply dialog, not in expressions. `registry_for(lib)` caches one registry per library `Arc` (a `Weak` key); `registry(ws)` replaces every `FunctionRegistry::default()` call site in `commands.rs` (MCP compiles through the same functions). `Bootstrap.functions` stays built-in only; the UI adds `custom_list().functions`, refreshed on `custom-changed`.
- **Formulas kept with an error:** `Formula.error` and `Formula::broken` (SQL `CAST(NULL AS VARCHAR)`, no totals). `restore_formulas` keeps a formula that doesn't compile or run, instead of dropping it. **Addition:** when the library changes, `publish` calls `refresh_formulas` on every workspace: each tab's formulas are compiled again with the new registry and the tab is rebuilt only if something differs, so formulas follow the current definition while the tab is open, and a deleted function's formula shows its error at once (header warning icon, the error in the formula bar, a status line). It holds the workspace lock while a changed tab's totals are recomputed, like `set_formula`.
- Expression bodies: `BodyLanguage::Expression`; `ValidateEnv::expression_sql` (a hook, since the core can't use the DSL) translates the body after the name and signature checks; the SQL then takes every check; `Compiled.hash` is the source's. The app resolves `[x]` against a catalog table `__pv_args` (typed by each parameter's first probe type) with the **built-in** registry only (one custom tool can't call another, so §6.6 still doesn't arise), strips the qualifier, and refuses an output still containing `__pv_` (XLOOKUP/VLOOKUP). `check_text` runs on the source first, so `__pv_` typed by the user is refused as before. Operations stay SQL.
- UI: the completion detail shows `custom · total · NUMERIC`; a bare `[` completes the formula's columns (and an expression body's parameters). **Found and fixed:** `.cm-tooltip { overflow: hidden }` clipped CodeMirror's completion info, so no function's doc popup had ever shown.

Phase 6, MCP and the assistant:

- `list_custom_tools` in both modes (and so the assistant, which uses the attached server in memory): name, kind, `Definition::signature()`, title, description, first example, and a ready-to-fill `apply_operation` / `set_formula` call.
- `propose_custom_tool { definition, replaces?, rationale?, offer? }`: the definition is read as JSON (`for_export`, so compiled/report/status are dropped), validated with `validate_in` against the user's folder (read only; tests use none), and the report returned in English (`custom::english`). Attached: `AttachHost::offer_custom` emits `custom-proposal`; the window asks, then opens the editor prefilled with the report and a note. **Deviation:** the proposal is offered whether it passes or not (the user may fix it), and `offer: false` lets the AI iterate without dialogs. It is not an approval card and never blocks the tool call. Headless only validates (`next` says so), with `--allow-write` too.
- Tests (`mcp.rs`): custom function, operation and summary formula through MCP; a proposal with `read_text` in a subquery refused by the guard (a bare scalar `read_text(...)` already fails to bind as a table function); nothing written headless; attached proposals offered, `offer: false` not.

### Implementation notes (phase 7)

- **`columns` parameters.** `ParamKind::Columns`; `min`/`max` are the count (min defaults to 1, at most 100); `CustomArg::Columns(Vec<String>)`; example args are `ExampleArg` (untagged: a value or a list), so old files read unchanged. **Rule** (compile.rs `mark_lists`): a columns parameter's reference must be a bare, unqualified, alias-free direct item of a select list (operations only), `GROUP BY`, `PARTITION BY`, `DISTINCT ON`, an `ORDER BY` item with default direction and NULLS order, the arguments of a non-operator function, or `COALESCE`'s children; otherwise `custom.check.columns_position`. The slot is filled with the quoted names joined by `, ` (no parentheses). Probes give it two columns named `<param>__1`, `<param>__2` (within its bounds; `__pv_` names are refused in results). Not callable from expressions or formulas: `function_def` returns `None` for such functions (deviation from §9.1's "column range" idea, which needs DSL changes). Samples `dedupe_by` (columns) and `percentile_value` (a Group By aggregate; `quantile_cont(x, coalesce(p, 0.9))` because probes pass NULL values and `quantile_cont` needs a constant).
- **Steps → operation** (`custom/record.rs`). Each query `Step` keeps a `Recipe { source, sql, op }`: the query it was built on (`None` for SQL mode), its SQL and the `Operation` (deviation from §4.4's "way 1" and "way 2": neither replay nor capture of the nested query; the step's own SQL with its source replaced). `source` appears verbatim in the step's SQL, so each step becomes `step_N AS (<sql with source → SELECT * FROM step_{N-1}>)`; the source is looked up by text, which also handles a sort replacing a sort. SQL-mode steps have the tab's name renamed to `t` in the tree. A step reading another table name, with no known source or no recipe, ends the chain (it and the later ones are listed as left out); cell edits, formulas and "Read as" are under the steps. Column parameters rename `COLUMN_REF`s (dropping a qualifier); a column that any step writes (alias, `* REPLACE/RENAME/EXCLUDE`) can't be one. Other referenced input columns become required columns (`number` for numeric families). `__pv_` becomes `_pv_`. The example is the first 10 (or 50) captured rows, its expected output computed by running the template. `Workspace::steps_capture` holds the workspace for one `LIMIT 50` read; `draft` runs on its own database. Known limit: steps that enumerate columns make every column required and drop the target's extra columns.
- **Custom aggregates in Group By.** `AggFunc::Custom` + `Aggregation.custom { name, values }`; `reshape::is_custom_aggregate` (usable summary function, one column parameter, the rest values). Pivot and charts refuse it.
- **Formula cells** set `allowOverlay: false` in `DataGrid.tsx`, so Glide's read-only overlay no longer opens over the formula bar; `onCellActivated` still opens the formula.
- **Formula refresh off the lock.** `refresh_formulas(&Mutex<Workspace>, tabs)`: compile under the lock; `Workspace::formula_totals_job` (a cloned connection, the edits view and a snapshot); `FormulaTotalsJob::resolve` computes the totals without the lock; `set_resolved_formulas` puts the resolved views in place only if the tab's formulas and edits are unchanged (else compile again, up to three times) and keeps the markers so later rebuilds compute totals again; when steps follow, the rows are counted off the lock too (`set_counted_rows`). Not a UI job (`run_job`): it runs in `publish`, not from a window, so there's no progress or cancel; the scan is still off the workspace lock.

---

## 1. Summary of the proposal

- Two user-facing things: a **custom function** (columns and values in, a column or one value out) and a **custom operation** (a table in, a table out).
- Each has a **typed signature**, a **body**, and **examples** (input data plus the expected output). The examples are test cases that must pass before it can be saved.
- The body is compiled **once**, at save time, to a **SQL template owned by ParVu**. Arguments are filled in later through `sql::quote_ident` / `sql::quote_literal`. DuckDB macros are not the runtime mechanism (§5.5).
- DuckDB itself is the type checker: the template is bound against empty, typed probe tables and the result type is read with `DESCRIBE`.
- Functions are written in the **expression language** (or a SQL expression). Operations are written as **one SQL `SELECT`**. "Save the tab's steps as an operation" comes later and generates that SELECT.
- Applying one is an ordinary `Operation`, so undo, the steps panel, Refresh, MCP and the assistant work with no special cases.

---

## 2. Concepts and names

| User-facing name | Takes | Returns | Where it is used |
|---|---|---|---|
| **Custom function**, row kind | columns, values | a column (one value per row) | Column menu, expressions, formulas |
| **Custom function**, summary kind | columns, values | one value (aggregate) | Column menu (shows the value), expressions, formula totals |
| **Custom operation** | a table, plus columns, values, other tables | a table | Rows / Table menus |

Notes on the naming:

- "Table → column" from the request is a row-kind function whose body reads other rows (a window, such as a rank within a group). It is not a third kind. The compiler detects it and sets a flag, `reads_other_rows`, with the same consequences as `FormulaSql::reads_neighbours` (`crates/parvu-dsl/src/lib.rs`).
- An operation states where its result goes: **changes this table** (a step, undoable) or **opens a new table** (a result tab). The definition decides, not the user at apply time. An operation with more than one table parameter always opens a new table, like Join and Append.
- Suggested UI words: "Custom function", "Custom operation", collectively "Custom tools" for the menu entry and the management list. Translation keys under `custom.*`.

---

## 3. Signature model

### 3.1 Parameter kinds

| Kind | Written | Meaning | Argument at apply time |
|---|---|---|---|
| column | `column<T>` | one column whose type is in `T` | a column name; inside an expression, any expression |
| columns | `columns<T>` with `min` / `max` count | a list of columns, each in `T` | a list of column names |
| value | `value<T>` | one constant | typed text, turned into a literal |
| table | `table` or `table{name: T, …}` | a table; listed columns must exist under exactly those names | the active tab (first table parameter) or another open tab |

Rules:

- A function has no table parameters. An operation has at least one; the first is "this table".
- A column parameter of an operation belongs to one table parameter (`of`, default the first).
- Optional parameters come last. Only `value` parameters can have a default. A default is text that must parse as the declared type.
- A value parameter may carry `choices` (a fixed list) and, for numbers, `min` / `max`.
- At most 12 parameters.

A table's required columns are **fixed names**: the body writes `t.amount`. A column parameter is a **flexible name**: the user picks the column. There is no remapping of required columns in v1.

### 3.2 Types

A type `T` is a **set of type families**. The family is the one ParVu already computes for every column: `TypeFamily` in `crates/parvu-core/src/types.rs` (`TypeFamily::of(dtype)`), which the UI has as `ColumnMeta.family`.

Named sets the signature builder offers:

| Name | Families |
|---|---|
| `any` | all |
| `number` | integer, decimal, float |
| `integer`, `decimal`, `float` | one each |
| `text` | text |
| `boolean` | boolean |
| `date` | date |
| `datetime` | timestamp, timestamp_tz |
| `date or datetime` | date, timestamp, timestamp_tz |
| `time`, `interval`, `json`, `list`, `struct` | one each |

Mapping to the other type vocabularies in the code:

| Use | Existing type | Mapping |
|---|---|---|
| Typed input box for a `value` parameter | `ValueKind` and `valueKind()` in `ui/src/dialogs/logic.ts` (number, date, datetime, time, boolean, text) | by family, exactly as `ConditionRows` does |
| Literal for a `value` argument | `ops::typed_literal(raw, dtype)` in `crates/parvu-core/src/ops.rs` | a value parameter stores one concrete DuckDB type (`BIGINT`, `DOUBLE`, `VARCHAR`, `DATE`, …) checked by `sql::is_safe_type_name` |
| Autocomplete and the doc popup in expressions | `LogicalType` in `crates/parvu-dsl/src/types.rs` | lossy: number → `Numeric`, text → `Text`, date → `Date`, datetime → `Timestamp`, a mixed set → `Unknown` |

### 3.3 Generics

Two forms, and no more:

1. **A family set is already a generic.** `column<number>` accepts BIGINT, DECIMAL and DOUBLE. Validation probes the body once per family in the set (§6.3).
2. **`same as <parameter>`** as the result type, e.g. `fill_zero(x: column<number>) -> column<same as x>`. Validation checks, for each probe, that the result family equals the argument's family.

No named type variables, no constraints between two parameters. If that is needed later, `same as` can be allowed on parameters.

### 3.4 Result

| Result | Declared as | Checked |
|---|---|---|
| column | `column<T>` or `column<same as p>` | row count unchanged, one output column, family in `T` |
| value | `value<T>` | the body is an aggregate: one row, one column |
| table | `table` or `table{name: T, …}` | listed columns are present with those families; other columns are allowed |

---

## 4. How the user defines one

### 4.1 What exists today

| Mechanism | Where | What it offers | Limits for this feature |
|---|---|---|---|
| Expression language | `crates/parvu-dsl` (parser → resolver → compiler) | Row expressions, aggregates, Excel functions, `[column]` references, autocomplete | No table-level constructs except `drop_duplicates` / `drop_null`. The resolver checks arity only: `check_args` says "Type checking is permissive". |
| Registry templates | `FunctionDef.sql_template` with `{0}`, `{1}`, and `ParamDef.default` (`registry.rs`, `compiler.rs` `templated`) | A function defined as SQL around its arguments: exactly a custom function | Placeholders `{N}` collide with regex quantifiers such as `'\d{2}'`. Fine for built-ins, not for user bodies. |
| Formulas | `crates/parvu-core/src/formula.rs`, `parvu_dsl::compile_formula` | Live computed columns; aggregates become totals computed once | Aggregates are a fixed list (`AGGREGATES` in `lib.rs`). |
| SQL mode | `Workspace::run_sql`, `run_relative_sql` | Anything DuckDB can do | No parameters. |
| Applied steps | `Step { label, kind }` in `workspace.rs`; `QueryEngine.history: Vec<String>` | Undo | A step stores a label and SQL text. **It does not store the `Operation`**, so steps cannot be replayed on another table today. |
| Read-only guard | `readonly::guard_select` | Parses with DuckDB (`json_serialize_sql`), allows one SELECT, rewrites table names, refuses file and table functions | Reusable as is, with a different `resolve` closure. |

### 4.2 Options compared

| Option | Power | Effort | Fits "compile once, validate strictly" | Verdict |
|---|---|---|---|---|
| A. Body in the expression language, parameters as `[name]` | Row and summary functions | Low: `resolve_in` with a synthetic table whose columns are the parameters. No parser change. | Yes | **Functions, default** |
| B. Body as a SQL expression | Everything DuckDB has (regex, lists, JSON) | Low | Yes | **Functions, advanced toggle** |
| C. Body as one SQL SELECT, parameters as table and column names | Any table operation | Medium | Yes | **Operations** |
| D. Record from applied steps | Familiar; no SQL to write | High if replayed as `Operation`s; medium if the tab's current SQL is captured | Partly (see §4.4) | **Later phase**, as a way to generate option C |
| E. Define by example only (synthesise the body from input → output) | Attractive | Very high; unreliable | No: no canonical body to check | **Rejected** as a definition method. Examples stay as tests. The assistant can propose a body from examples (§9.3). |

### 4.3 Recommendation

- **Function body:** expression language by default, SQL expression as a switch in the editor. Both produce a SQL expression in which parameters are column references, and both go through the same compiler in `parvu-core`.
- **Operation body:** one SQL `SELECT` that reads its table parameters by name.
- **Examples are part of the definition** and are required: at least one per definition.

How parameters appear in a body:

| Body language | A parameter `phone` is written | A table parameter `t` |
|---|---|---|
| Expression | `[phone]` | n/a |
| SQL expression | `phone` | n/a |
| SQL SELECT | `phone` (or `t.phone`) | `FROM t` |

### 4.4 "Save steps as an operation" (later)

Two ways to do it; both generate an option-C definition that the user then reviews, and both go through the same validation.

1. **Capture the SQL.** Take `engine.current_query()` and rename the tab's base table to the table parameter. Cheap. The generated SQL is nested and hard to read, and `run_relative_sql` wraps steps as `WITH "sales" AS (…)`, which shadows the table name and needs scope-aware renaming. Column and value parameters are added by hand afterwards.
2. **Record the operations.** Add `recipe: Option<Operation>` to `Step` and replay with `Operation::build` on the target table. Robust for menu operations, but SQL-mode and expression steps have no `Operation`, and some operations read data while building (pivot values, bin edges through `OpContext::query`), so the result is not one fixed SQL template. That conflicts with "compiled once".

Recommendation: way 1, only for tabs whose steps are all menu operations or expressions, and only after phases 1–3 prove the model. Cell edits and formulas are not captured (they live below the steps).

### 4.5 Examples (built-in tests)

An example is:

- **Inputs.** For a function: a small grid with one column per column parameter (typed by a concrete DuckDB type), plus the value arguments. For an operation: one small table per table parameter, plus column and value arguments.
- **Expected output.** A column of values, one value, or a table.
- **Options.** `ordered` (tables only; default off: compared as a multiset), `float_digits` (default 9 significant digits).

Limits: 20 examples, 200 rows and 50 columns each. Cells are stored as text or null and cast to the column's declared type, the same way cell values travel today (`page::Cell`).

The editor has a **Fill expected from result** button: it runs the body on the inputs and writes the output into the expected grid. The user reads it and confirms. That is the practical form of "define by example": the user supplies inputs, sees outputs, and pins them.

The validator warns (does not fail) when no example contains an empty (NULL) input.

---

## 5. Compilation

### 5.1 Pipeline

Run once, on Save (and again on import or after an app update, §7.4).

```
body source
  │  expression body: parvu_dsl::resolve_in(ast, catalog{__pv_args: params}, registry, "__pv_args") → compile
  ▼
SQL text with parameters as column refs            (parvu-app: the only crate that sees both dsl and core)
  │  json_serialize_sql                            (DuckDB's parser, as readonly::guard_select does)
  ▼
syntax tree (JSON)
  │  walk: safety guard, determinism, references;
  │  rename each parameter reference to the reserved identifier __pv_arg_N,
  │  each table parameter to __pv_tab_N
  │  json_deserialize_sql
  ▼
canonical template (SQL text)                      stored in the definition file as `compiled.template`
```

The template is plain SQL in which `__pv_arg_0`, `__pv_arg_1`, … stand for the arguments. User bodies may not contain the text `__pv_` at all (ParVu already reserves that prefix: `__pv_rn`, `__pv_src_…`, `__pv_res_N`), so every occurrence of `__pv_arg_N` in the template is a slot the compiler wrote. Slot replacement is therefore exact.

Function bodies are compiled by wrapping the expression as `SELECT <expr> FROM __pv_args`, then unwrapping the select item after the walk.

### 5.2 Binding arguments safely

No user text is ever pasted into SQL unquoted. Each slot takes one of:

| Argument | SQL written into the slot | Rule |
|---|---|---|
| Column (from a dialog, MCP or the assistant) | `quote_ident(name)`; qualified with the table alias where the body qualified it | `ctx.require(name)` first (`OpContext` in `ops.rs`) |
| Columns list | `quote_ident(a), quote_ident(b), …` | allowed only in list positions, see below |
| Value | `ops::typed_literal(raw, dtype)`: a checked number, or `CAST(quote_literal(raw) AS <dtype>)` with `is_safe_type_name(dtype)` | the dtype comes from the saved signature, never from the caller |
| Expression (a call inside the expression language) | the compiled argument in parentheses, as `Compiler::templated` already does | the argument was compiled by `parvu-dsl` |
| Table | `WITH "__pv_tab_0" AS (<the tab's current query>) <template>`, the pattern of `Workspace::run_relative_sql` | other tables through `OpContext::other_query` |

A slot is replaced with the argument in parentheses. A parameter used twice in the body evaluates its argument twice, which is harmless because volatile functions are refused (§6.5).

**Column lists.** A `columns` parameter is valid only where SQL takes a comma-separated list: the select list, function arguments, `GROUP BY`, `PARTITION BY`, `ORDER BY`. The compiler checks this on the syntax tree: the slot's parent must be an array. `cols + 1` is refused with "A column list can be used only where several columns are allowed". In expression bodies a `columns` parameter behaves as a column range, which the resolver already spreads for `SUM([q1:q4])` (`spread_args`, `ROW_FUNCS`).

### 5.3 How an application becomes SQL

Two new variants of `parvu_core::ops::Operation`. They carry names and structured arguments, never SQL:

```rust
/// A custom function of columns, written in place or into a new column.
CustomFunction {
    name: String,
    args: BTreeMap<String, CustomArg>,
    #[serde(default)] output: Output,                      // colfn::Output: InPlace | New { name }
    #[serde(default, rename = "where")] filter: Option<RowFilter>,
},
/// A custom operation. `new_table` must equal what the definition says.
CustomOperation { name: String, args: BTreeMap<String, CustomArg>, #[serde(default)] new_table: bool },

pub enum CustomArg { Column(String), Columns(Vec<String>), Value(String), Table(String) }
```

`Operation::build` needs the compiled definition. `parvu-core` cannot read `~/.ParVu`, so `OpContext` gets one more lookup, next to `other_query`:

```rust
pub custom: &'a dyn Fn(&str) -> Option<Arc<custom::Compiled>>,
```

The `Workspace` holds an `Arc` snapshot of the compiled library, set by the app (`Workspace::set_custom`, like `set_detect_options`).

| Applied thing | Built SQL | Path through the code |
|---|---|---|
| Row function, in place | `SELECT * REPLACE (<filled template> AS "col") FROM (<cur>) AS "tab"` | `Workspace::apply` → one `Step`. Same shape as `Operation::AddColumn`. `clean::only_where` handles the optional row filter. |
| Row function, new column | `SELECT *, <filled template> AS "new" FROM (<cur>) AS "tab"`; name through `ops::unique_name` | same |
| Operation that changes this table | `WITH "__pv_tab_0" AS (<cur>) <filled template>` | `Workspace::apply` → one `Step` |
| Operation that opens a new table | same SQL | `is_table_op()` returns `new_table` → `Workspace::plan_result(Derivation::Op { op })` → `workspace::materialize` as a job (`make_table` in `commands.rs`) → `add_result` |
| Summary function from the menu | `sql::whole_table(<filled template> AS v, <cur>, alias)` with the `whole_table_plain` fallback, as `sql::query_whole_table` does | not an `Operation` (nothing changes). A new command `custom_value` run through `jobs::run_job`; the UI shows the value. |
| Any function inside an expression or formula | the template, filled by the DSL compiler | §9.1 |

`is_table_op(&self)` has no context, which is why `new_table` travels in the call. `build` refuses a call whose `new_table` differs from the definition.

`result_suffix()` for a custom operation is the definition's name, so the tab is called `sales_top_n_per_group`.

### 5.4 Undo, Refresh, step label

- **Undo.** A step keeps its SQL text in `QueryEngine.history`. Editing or deleting a definition later does not affect steps already applied. This is snapshot behaviour and needs no code.
- **Refresh.** A result tab's `Origin.derivation` is `Derivation::Op { op }`. `refresh_plan` builds it again, so Refresh uses the **current** definition. If the definition was deleted or is broken, Refresh fails with "Custom operation top_n_per_group no longer exists"; the tab keeps its rows.
- **Formulas** are stored as source text and compiled again on open (`column_types.rs`, `SavedFormula`). A formula that calls a custom function therefore follows the current definition. If that fails on open, the formula is dropped as today; this should be changed to keep the formula and show the error (§11.1).
- **Step label.** `StepLabel::new("step.custom", &[("name", "clean_phone"), ("args", "Phone")])`, shown as "clean_phone (Phone)". `args` is the comma-joined column and value arguments, cut at 60 characters like `step.query`.

### 5.5 DuckDB macros: assessed, not used as the mechanism

What macros offer (from memory, **[verify]** each):

- `CREATE [OR REPLACE] [TEMP] MACRO f(a, b := 1) AS <expr>` and `… AS TABLE <select>`.
- Expansion happens when the query is bound, so there is no run-time cost.
- Parameters are untyped in the versions I know well. Newer versions may accept type annotations on parameters; I am not sure the version in the tree (`duckdb = "1.20000.1"`) has them. There is no declared result type in either case.
- A table macro's parameters are expressions, not relations. A table is passed by name through `query_table(name)`.
- No recursion in scalar macros. Overloading by argument count exists in recent versions.

Why the template approach is better for ParVu:

| Concern | Macro | ParVu template |
|---|---|---|
| Types | none, or parameters only | checked by our probes either way |
| Table parameters | awkward (`query_table` with a string) | CTE, as `run_relative_sql` already does |
| Step SQL stays valid after the definition changes | no: the step would call the macro by name, so undo history and result tabs break or silently change | yes: the SQL is self-contained |
| Connections | jobs and `ReadSession` run on cloned connections; whether a macro is visible there depends on TEMP vs catalog scope **[verify]** | no catalog state |
| Read-only guard | `json_serialize_sql` shows a macro call as a function name; the guard cannot see the body | the guard sees everything |
| Column lists, typed literals | not expressible | handled by the filler |

One later use for macros: making custom functions callable **in SQL mode**. A row or summary function maps directly to `CREATE OR REPLACE MACRO name(p0, p1) AS (<template>)` on the workspace's database. That is optional (§11.3, question 5).

---

## 6. Strict validation before saving

Validation lives in `parvu-core/src/custom/validate.rs`. It runs on its **own in-memory DuckDB** (`Connection::open_in_memory()`, `SET memory_limit = '256MB'`, `SET threads = 2`), never on the user's workspace, so it holds no workspace lock and cannot touch user data. It runs on the blocking pool with an interrupt handle and a 5-second limit per example (the pattern of `ReadSession::query`). **[verify]** that `json_serialize_sql` is available on a fresh in-memory database in the prebuilt build; if not, use `Workspace::clone_connection()` with TEMP tables.

The Save command runs the full validation itself and writes the file only when it passes. The UI's Validate button calls the same function without writing. A definition that does not pass can be kept as a **draft** (`status: "draft"`): stored, listed, not usable.

The result is a report: a list of checks, each with a status (pass / warning / fail / skipped), a message as translation key plus arguments (the `StepLabel` shape), and where possible a position in the body.

### 6.1 The checks

| # | Check | Fails when |
|---|---|---|
| 1 | **Name** | Not `[a-z][a-z0-9_]{0,39}`; a built-in of the expression language (`FunctionRegistry::default().lookup`), an Excel function (`excel::is_excel`), a resolver special form (`DROP_DUPLICATES`, `DROP_NULL`, `REPLACE`, `XLOOKUP`, `VLOOKUP`, `TEXT`, `PREV`, `NEXT`), a DuckDB function (`duckdb_functions()`), a reserved word (`RESERVED_KEYWORDS` in `compiler.rs`), or another custom definition. Names compare case-insensitively. |
| 2 | **Signature** | Duplicate or reserved parameter names; a parameter named like a required column; optional before required; a default that does not parse as its type; a function with a table parameter; an operation without one; more than 12 parameters. |
| 3 | **Body parses** | Empty; over 20,000 characters; contains `__pv_` or control characters; the expression language or DuckDB's parser rejects it; more than one statement; not a SELECT (operations). Syntax the serializer cannot round-trip is refused with "not supported in custom operations" **[verify]** which constructs those are. |
| 4 | **References** | A name in the body is not a parameter, a required column, or something the body defines. Found by binding against the probe tables (§6.3): DuckDB's binder is the authority. An alias that shadows a parameter fails. An unused parameter is a warning. |
| 5 | **Safety** | §6.2 |
| 6 | **Determinism** | §6.5 |
| 7 | **Types** | For some probe the body does not bind, or the result family is not the declared one (§6.3). |
| 8 | **Shape** | A row function that aggregates; a summary function that does not; an operation whose result has duplicate column names or `__pv_` columns (§6.4). |
| 9 | **Dependencies** | A call to a custom function that does not exist, is a draft or is broken; a cycle; nesting deeper than 8; an expanded template over 64 KB (§6.6). |
| 10 | **Resources** | Warnings only (§6.7). |
| 11 | **Examples** | No example; an example's arguments do not fit the signature; any example's output differs from the expected output. The report shows a cell-level diff. |
| 12 | **Stable compile** | Compiling twice gives different templates (a guard against serializer surprises). |

### 6.2 Safety guard

Reuse `readonly::guard_select(conn, sql, resolve)` unchanged, with `resolve` returning a name only for the definition's table parameters. That gives, with no new code:

- one SELECT only: no DDL, no INSERT / COPY / ATTACH / SET;
- no table functions except `range`, `generate_series`, `unnest`, so no `read_csv`, `read_text`, `glob`;
- no file paths as tables, no schema-qualified names;
- no reading of other open tabs: an unknown table is an error;
- no `getenv`, `current_setting` (`DENIED_FUNCTIONS`);
- a CTE may not reuse a table parameter's name.

Additions for this feature:

- A function body may not contain a subquery or any table reference at all.
- The function check of §6.5.
- Imported files and AI proposals are compiled from their source; a `compiled` block in an incoming file is ignored.

### 6.3 Type check and result inference

DuckDB binds the template against empty typed inputs; nothing is executed.

1. For each column parameter choose representative types per family: integer → `BIGINT`, decimal → `DECIMAL(18,3)`, float → `DOUBLE`, text → `VARCHAR`, date → `DATE`, datetime → `TIMESTAMP` and `TIMESTAMP WITH TIME ZONE`, and so on. `any` probes a fixed spread (BIGINT, DOUBLE, VARCHAR, DATE, BOOLEAN).
2. Build the probe relation: `SELECT CAST(NULL AS <type>) AS "__pv_probe_0", … WHERE false`. Table parameters get their required columns plus the probe columns of the column parameters that belong to them. Value parameters are filled with a typed `NULL` cast, and also with their default.
3. Fill the template and call `engine::describe` (`DESCRIBE`, no execution).
4. Compare `TypeFamily::of(result dtype)` with the declared result; for `same as p`, with the probe type of `p`.

The number of probes is the product of family counts. Cap it at 64; above that, probe each parameter's families one at a time with the others fixed, and say so in the report.

The inferred result type per probe is stored (`compiled.result_types`) and shown in the editor ("number → DOUBLE, decimal → DECIMAL(38,3)").

### 6.4 Shape check

Bind one more probe against a relation that also has a row-number column: `SELECT <body>, "__pv_rn" FROM probe`.

- For a row function it must bind. An aggregate fails here ("must appear in the GROUP BY clause").
- For a summary function it must **fail** to bind, and `SELECT <body> FROM probe` alone must bind.

A window in a row function (`… OVER (…)`) sets `reads_other_rows`.

### 6.5 Determinism

Every function node in the syntax tree is looked up in `duckdb_functions()`. **[verify]** that this table has the `stability` and `has_side_effects` columns in the shipped version; if not, fall back to a fixed list (`random`, `setseed`, `uuid`, `gen_random_uuid`, `nextval`, `currval`, `now`, `current_timestamp`, `current_date`, `today`, `current_time`, `get_current_timestamp`, `transaction_timestamp`).

| Finding | Result |
|---|---|
| Volatile or side-effecting function | fail |
| Stable only within a query (`now()`, `current_date`, the expression language's `TODAY`, `NOW`) | fail unless the author ticks "Depends on today's date" (`flags.uses_current_time`); shown as a badge in the list |
| Order-dependent constructs: a window with no `ORDER BY`, `LIMIT` without `ORDER BY`, `first` / `last` / `any_value` / `string_agg` / `list` without an order | warning "Depends on row order". Not a failure: ParVu itself relies on table order (`ops::with_row_number`). |

Ties in an `ORDER BY` cannot be detected statically; the documentation should say so.

### 6.6 Cross-references and recursion

- A custom function may call other custom functions. They are expanded into the template at compile time, so the saved template is self-contained.
- Recursion is impossible to expand, so the dependency graph must have no cycle.
- `compiled.depends_on` records each dependency's name and content hash.
- Saving a changed definition re-validates everything that depends on it, including their examples. If a dependent would break, the save is refused and the report names it. Deleting a definition that others use is refused the same way.
- Operations may call custom functions. Operations cannot call operations in v1.

### 6.7 Resource limits

- **Validation itself** is bounded: own database, 256 MB, example size limits, 5 seconds per example.
- **Summary functions** applied from the menu always go through `sql::whole_table` (the constant `GROUP BY` that lets DuckDB spill, pitfall 1 in `AGENTS.md`) and run as a job with cancel. Inside a formula they become totals, which `formula::with_totals` already computes through `sql::query_whole_table`.
- **Operation bodies** are the author's SQL. The compiler finds SELECT nodes that aggregate with no `GROUP BY` and use a holistic aggregate (`mode`, `median`, `quantile*`, `count(DISTINCT …)`, `list`, `string_agg`) and warns "May need a lot of memory on a big table". It does not rewrite the query: adding a constant group changes the result on an empty table (no row instead of one).
- **Operations that open a new table** run in `materialize` as a job, capped by `RESULT_ROW_LIMIT`, with the `TooBig` path to a file. **Operations that change the table** are lazy SQL like every other step.
- `reads_other_rows` functions used in a formula are subject to the existing `NEIGHBOUR_MAX_ROWS` rule in `commands.rs` (`compile_tab_formula`).
- The memory guard (`parvu-app/src/memguard.rs`) stays the last line of defence.

### 6.8 What is checked again at use time

Saved validation proves the definition is sound for its declared types. Each application still checks the actual table, in `Operation::build` and then by DuckDB:

| Check | How |
|---|---|
| The definition exists and is usable (not draft, not broken) | `OpContext::custom` |
| Every required argument is present; no unknown argument names | against the signature |
| Each column exists | `ctx.require` |
| Each column's family is in the parameter's set | `ColumnMeta.family` |
| Column list length within `min` / `max` | signature |
| Each value parses as its type and respects `min` / `max` / `choices` | `ops::typed_literal` |
| Each table argument is an open tab with the required columns and families | `ctx.other_query`, `engine::describe` |
| Output column name is not empty and does not collide (new column) | `ops::unique_name` |
| The built SQL binds | `engine::validate` (`LIMIT 0`), which `set_query` and `plan_result` already call |
| The result family matches the declaration | `engine::describe` on the built SQL, before `set_query` |

Errors are `CoreError::Invalid` with a message naming the parameter, surfaced as `ApiError { kind: "custom", … }`.

Data errors (a cast that fails on row 1,000,000) appear when the page is fetched, as for any step. Authors should prefer `TRY_CAST`; the validator warns on a plain `CAST` from text.

---

## 7. Storage and sharing

### 7.1 Where

`~/.ParVu/custom/<name>.json`, one file per definition, like `~/.ParVu/themes/`. One file each makes sharing a matter of copying a file and keeps one corrupt file from hiding the rest.

- `AppPaths::custom_dir()` in `crates/parvu-config/src/paths.rs`.
- The store itself in `crates/parvu-app/src/custom_store.rs`. The definition types need DuckDB-side validation and so live in `parvu-core`; `parvu-config` cannot depend on `parvu-core`. The app already stores `column_types.json` and `providers.json` this way.
- Writes through `write_atomic` (`parvu-config/src/error.rs`).
- An unreadable file is logged and skipped, the behaviour of `RecentsManager::with_file`. It is listed in the manager as "could not be read".

### 7.2 File format

```json
{
  "schema": 1,
  "kind": "function",
  "name": "clean_phone",
  "title": "Clean phone number",
  "description": "Keeps digits only; too short becomes empty.",
  "status": "ok",
  "params": [
    { "name": "phone",  "kind": "column", "type": ["text"] },
    { "name": "prefix", "kind": "value",  "dtype": "VARCHAR", "default": "+" }
  ],
  "result": { "kind": "column", "type": ["text"] },
  "body": { "language": "sql", "source": "CASE WHEN … END" },
  "flags": { "uses_current_time": false },
  "examples": [
    {
      "name": "typical",
      "columns": [ { "param": "phone", "dtype": "VARCHAR" } ],
      "rows": [ ["+1 (555) 010-2030"], ["n/a"], [null] ],
      "values": { "prefix": "+" },
      "expected": { "dtype": "VARCHAR", "values": ["+15550102030", null, null] }
    }
  ],
  "compiled": {
    "template": "CASE WHEN … __pv_arg_0 … END",
    "hash": "sha256 of name + params + result + body + flags",
    "shape": "row",
    "reads_other_rows": false,
    "result_types": { "text": "VARCHAR" },
    "depends_on": [],
    "app": "1.1.0",
    "duckdb": "v2.0.x"
  },
  "created": "2026-11-17T10:00:00Z",
  "modified": "2026-11-17T10:00:00Z"
}
```

- `schema` is the format version. A file with a higher number than the app knows is listed as "made by a newer ParVu" and left untouched. Lower numbers are migrated in memory and rewritten on the next save.
- `compiled` is a cache. It is trusted only when `hash` matches the source parts **and** `app` and `duckdb` equal the running versions.
- Operations add `"placement": "step" | "new_table"` and table parameters with `"columns": [{ "name", "type" }]`.
- `sha2` is already in `Cargo.lock`; check whether it is a direct dependency before adding one.

### 7.3 Import and export

- **Export:** one definition → its file, with `compiled` stripped. Several, or one with dependencies → a bundle `{ "schema": 1, "definitions": [ … ] }` saved as `*.parvu-custom.json`.
- **Import:** each definition is compiled and fully validated, examples included, as if typed in. A name that exists offers Replace / Keep both (rename) / Skip. Dependencies in the bundle are imported first. A definition that fails is imported as a draft with its report, so the user can fix it.
- A shared file is safe to import because of §6.2: it cannot read files or other tabs.

### 7.4 When a definition breaks after an update

On start, in the background (not on the main thread, no workspace lock):

1. Load every file. For those whose `compiled.app` or `compiled.duckdb` differ from the running versions, compile and validate again.
2. Pass → rewrite `compiled`. Fail → `status: "broken"` with the report saved beside it. The source is never changed or deleted.
3. A broken definition is not registered: it is absent from menus and autocomplete, and calls fail with "Custom function X needs attention". The manager shows it with a warning and opens the editor on the failing check.
4. If any broke, show one notice after start: "2 custom tools need attention".

The examples are what make this check meaningful: a DuckDB upgrade that changes a function's behaviour shows up as a failed example, not as silently different data.

Definitions are app-wide. A change emits a `custom-changed` event (the pattern of `theme-changed`, `recents-changed` in `commands.rs`) so every window refreshes its menus and autocomplete, and each window's `Workspace` receives the new snapshot.

---

## 8. UI

All text through `t()`, keys added to `en.json`, `ru.json`, `az.json`. Colours from `--pv-*` / `--md-*`. Menus through `PopupMenu`. One file per dialog in `ui/src/dialogs/`, opened with `openXxx(ui, args)`, pure logic in `dialogs/logic.ts`.

### 8.1 Menus (`ui/src/App.tsx`)

| Menu | Addition |
|---|---|
| **Column** (and the column context menu, `columnMenu`) | Submenu **Custom functions**: the functions whose first column parameter accepts the selected column's family, then "New custom function…". Clicking one opens the apply dialog with that column filled in. |
| **Rows** | After the duplicates group: custom operations with `placement: step`. |
| **Table** | After Join / Append: custom operations with `placement: new_table`, called through `runOpDialog(open, undefined, true)` like Group By. |
| **Table** (bottom) | "New custom operation…" and "Manage custom tools…". |
| Steps panel (later phase) | "Save steps as custom operation…". |

Lists longer than 12 entries collapse to "More…", which opens the manager.

### 8.2 Editor dialog — `CustomEditorDialog.tsx`

One large dialog, four areas:

1. **Header:** kind (function / operation), name, title, description.
2. **Signature builder:** a row per parameter: name, kind, type (named sets from §3.2; a concrete type for values), optional and default, table columns. Below it the result: kind and type, and for operations the placement. A read-only line shows the signature as text: `clean_phone(phone: column<text>, prefix: value<text> = "+") → column<text>`.
3. **Body:** CodeMirror 6. For expression bodies reuse the expression editor's language and completer (`ui/src/lib/exprLanguage.ts`, `exprComplete.ts`) with the parameters offered as `[name]`. For SQL bodies use the SQL editor setup of `components/QueryEditor.tsx` with parameters and table columns added to the completion list. A switch selects the language for functions. A collapsed "Compiled SQL" section shows the template after a successful compile.
4. **Examples:** tabs, one per example. Each has small editable grids (inputs, expected) and inputs for value arguments, typed by `ValueKind`. Buttons: Add row, Fill expected from result, Duplicate example. After a run, differing cells are marked and show expected vs actual.

Footer: **Validate** (runs §6, shows the check list with pass / warning / fail icons; clicking a failed check focuses its place), **Save draft**, **Save** (enabled only when the last validation of the current text passed; the backend validates again regardless).

A plain HTML table is enough for the example grids; Glide Data Grid is unnecessary at 200 rows.

### 8.3 Apply dialog — `CustomApplyDialog.tsx`

Generated from the signature. No per-definition UI code.

| Parameter | Control |
|---|---|
| column | `ColumnSelect` (`dialogs/common.tsx`) listing only columns whose family fits; the selected column preselected |
| columns | the multi-column picker used by Group By / Value Counts, filtered by family, with the count limits |
| value | the `ValueInput` of `ConditionRows.tsx`, chosen by `valueKind`; a select when `choices` is set; the default prefilled |
| other table | a select of open tabs that have the required columns, as `JoinDialog` |

Plus, for row functions: output (in place / new column with a name), the same control as `ColumnFnDialog`, and the optional "Only rows where…" block (`ConditionRows`). A preview of the first rows, through the existing `Workspace::preview_op`. For summary functions the dialog runs the job and shows the value with a Copy button.

The dialog returns an `Operation`; `logic.ts` gets `buildCustomOp(def, form)` and `customArgProblems(def, columns, form)` with tests.

### 8.4 Manager — `CustomToolsDialog.tsx`

A list with name, signature, kind, status badge (ok / draft / needs attention / depends on today's date), and last modified. Actions: New, Edit, Duplicate, Delete (refused while used by another definition), Import, Export, Run examples for all.

---

## 9. Integration

### 9.1 Expression language and formulas

Today every call site builds `FunctionRegistry::default()` (`commands.rs`: `bootstrap`, `run_expression`, `check_target_table`, `compile_tab_formula`, expression preview). Replace those with one accessor on `AppState` that returns the built-in registry plus one `FunctionDef` per usable custom function, cached and rebuilt on change. `Bootstrap.functions` then carries them to the UI, so autocomplete and the doc popup work with no UI change beyond refreshing on `custom-changed`.

Changes needed in `parvu-dsl` (it stays free of DuckDB):

| Change | Why |
|---|---|
| `fill_template` accepts `__pv_arg_N` slots in addition to `{N}` (a field on `FunctionDef` selects the style) | `{2}` in a user's regex must not be read as a slot |
| `FunctionDef` gains `aggregate: bool` and `window: bool` | `compile_formula` decides totals by the fixed `AGGREGATES` list and neighbours by `template.contains(" OVER (")` on `Expr::Template` only |
| `ParamDef` gains an optional list of accepted `LogicalType`s; the resolver reports a mismatch when the argument's type is known and not accepted | strict typing for custom functions; built-ins keep the permissive behaviour |
| Optional value parameters use the existing `ParamDef::with_default` | the default is the typed literal |

`columns` parameters are not callable from expressions in the first version (call with explicit arguments from the dialog instead).

The built-in registry is unchanged, so `crates/parvu-dsl/tests/golden/dsl.json` stays valid. New golden cases use a registry with test custom functions.

### 9.2 MCP and the assistant

- **Using.** `apply_operation` takes an `Operation`, and its JSON schema comes from `operation_schema()`. The new variants appear there once `CustomArg` derives `JsonSchema` under the `schema` feature. Add a read tool `list_custom_tools` (name, signature, description, one example) to `mcp.rs` and to the assistant's tool list in `ai_assistant.rs`, and name it in the server instructions.
- **Guard.** `ReadSession::check_operation` (`readonly.rs`) gets a branch: custom calls carry no SQL, so it only checks that table arguments are open tabs. The template was guarded when it was saved.
- **Headless `parvu mcp`.** `ParvuMcp::new` must load the library from `~/.ParVu/custom` and hand it to its `Workspace`, otherwise the variants fail there.
- **Approval cards** already show the SQL and a preview (`Proposal` in `mcp.rs`, `McpProposalDialog.tsx`); a custom call needs nothing new.

### 9.3 Should AI be able to define them?

Recommendation: **propose, not save.**

- A new tool `propose_custom_tool` takes a full definition (signature, body, examples). ParVu validates it with §6 and returns the report to the model, so it can correct itself.
- In the window (attached mode and the built-in assistant), a passing proposal opens the editor dialog prefilled, with the validation results. The user saves it. Nothing is written without that.
- Headless `parvu mcp` cannot define in v1, even with `--allow-write`: a definition is persistent state in `~/.ParVu` that outlives the session.

A definition changes what future sessions do, which is a different category from a step the user can undo. The examples are the user's check on an AI-written body: they can read three rows of input and output without reading SQL.

---

## 10. Worked examples

The template text below shows the shape; the exact spelling is whatever `json_deserialize_sql` prints.

### 10.1 Row function: `clean_phone`

**Signature:** `clean_phone(phone: column<text>, prefix: value<text> = "+") → column<text>`

**Body** (SQL expression; the expression language has no regex function today):

```sql
CASE WHEN length(regexp_replace(phone, '[^0-9]', '', 'g')) < 7 THEN NULL
     ELSE prefix || regexp_replace(phone, '[^0-9]', '', 'g') END
```

**Examples**

| phone (VARCHAR) | prefix | expected |
|---|---|---|
| `+1 (555) 010-2030` | `+` | `+15550102030` |
| `555.0102` | `+` | `+5550102` |
| `n/a` | `+` | empty |
| empty | `+` | empty |

**Validation highlights:** safety passes (no tables, only `length`, `regexp_replace`, `||`); types: probe `VARCHAR` → result `VARCHAR`; shape: row; four examples pass.

**Compiled template**

```sql
CASE WHEN (length(regexp_replace(__pv_arg_0, '[^0-9]', '', 'g')) < 7) THEN NULL
     ELSE (__pv_arg_1 || regexp_replace(__pv_arg_0, '[^0-9]', '', 'g')) END
```

**Applying it.** The user selects a cell in `Phone`, Column → Custom functions → clean_phone. The dialog shows `phone = Phone`, `prefix = +`, output "New column: Phone_clean". The UI sends:

```json
{ "op": "custom_function", "name": "clean_phone",
  "args": { "phone": { "column": "Phone" }, "prefix": { "value": "+" } },
  "output": { "to": "new", "name": "Phone_clean" } }
```

`Operation::build` produces (tab `contacts`):

```sql
SELECT *, CASE WHEN (length(regexp_replace(("Phone"), '[^0-9]', '', 'g')) < 7) THEN NULL
               ELSE (('+') || regexp_replace(("Phone"), '[^0-9]', '', 'g')) END AS "Phone_clean"
FROM (<current query>) AS "contacts"
```

Step label: "clean_phone (Phone)". Undo removes the step.

In the expression editor: `contacts[mobile] = clean_phone(contacts[Phone])`. As a formula: `= clean_phone([Phone], "00")`.

### 10.2 Operation: `top_n_per_group`

**Signature:** `top_n_per_group(t: table, grp: column<any>, score: column<number>, n: value<integer> = 3, min 1) → table`, placement: changes this table.

The parameters are `grp` and `score` because `group` and `by` are reserved words; check 2 refuses those names.

**Body**

```sql
SELECT * FROM t
QUALIFY row_number() OVER (PARTITION BY grp ORDER BY score DESC NULLS LAST) <= n
```

**Example** (`grp = region`, `score = revenue`, `n = 2`, compared unordered)

Input `t`:

| region | revenue |
|---|---|
| east | 10 |
| east | 30 |
| east | 20 |
| west | 5 |

Expected:

| region | revenue |
|---|---|
| east | 30 |
| east | 20 |
| west | 5 |

**Validation highlights:** the guard resolves `t` only; probes bind `grp` as each of five types and `score` as BIGINT, DECIMAL, DOUBLE (15 probes); result is a table with the input's columns; determinism passes (the window has an `ORDER BY`). Equal scores are split arbitrarily, which the checks cannot see; `rank()` would keep ties.

**Compiled template**

```sql
SELECT * FROM __pv_tab_0
QUALIFY (row_number() OVER (PARTITION BY __pv_arg_0 ORDER BY __pv_arg_1 DESC NULLS LAST) <= __pv_arg_2)
```

**Applying it.** Rows → top_n_per_group. The dialog offers every column for `grp`, numeric columns for `score`, and a number box for `n`. The UI sends:

```json
{ "op": "custom_operation", "name": "top_n_per_group",
  "args": { "grp": { "column": "region" }, "score": { "column": "revenue" }, "n": { "value": "3" } } }
```

Built SQL, applied as one step by `Workspace::apply`:

```sql
WITH "__pv_tab_0" AS (<current query>)
SELECT * FROM __pv_tab_0
QUALIFY (row_number() OVER (PARTITION BY ("region") ORDER BY ("revenue") DESC NULLS LAST) <= (3))
```

Had the definition said "opens a new table", the same SQL would go through `plan_result` → `materialize` → `add_result`, the tab would be named `sales_top_n_per_group`, and Refresh would rebuild it from `Derivation::Op`.

### 10.3 Summary function: `weighted_avg`

**Signature:** `weighted_avg(x: column<number>, w: column<number>) → value<float>`

**Body** (expression language): `SUM([x] * [w]) / SUM([w])`

**Examples**

| x | w | expected |
|---|---|---|
| 10, 20, 30 | 1, 1, 2 | 22.5 |
| 10, empty | 1, 5 | 1.666666667 |

The second example pins a real property of this body: a row with an empty `x` still counts its weight. An author who expected 10 sees the example fail and changes the body. That is the purpose of required examples.

**Validation highlights:** the first shape probe fails to bind, as it must for an aggregate; nine type probes. Whether integer ÷ integer gives a float in DuckDB (`/` is float division, to my knowledge) is settled by the probe, not by assumption: if a probe returns an integer family the type check fails and names it.

**Compiled template:** `(sum((__pv_arg_0 * __pv_arg_1)) / sum(__pv_arg_1))`, `shape: "summary"`.

**Applying it**

- Column → Custom functions → weighted_avg, with `x = price`, `w = qty`. The command `custom_value` runs as a job:

  ```sql
  SELECT (sum((("price") * ("qty"))) / sum(("qty"))) AS v
  FROM (SELECT *, 0 AS __pv_one FROM (<current query>) AS "sales") AS "sales" GROUP BY __pv_one
  ```

  (`sql::whole_table`; on an empty table it falls back to `whole_table_plain`.) The dialog shows the value. Nothing in the tab changes, so there is no step.
- In a formula: `= [price] - weighted_avg([price], [qty])`. `compile_formula` sees `aggregate: true`, makes it a total, and `formula::with_totals` computes it once per rebuild.

---

## 11. Plan, risks, open questions

### 11.1 Phases

| Phase | Delivers | Files | Tests |
|---|---|---|---|
| **1. Core model and compiler** | Types, compile to template, safety guard, type and shape probes, example runner, report | new `crates/parvu-core/src/custom/{mod,signature,compile,validate,examples,fill}.rs`; `lib.rs`; small visibility changes in `readonly.rs` (tree helpers), `engine.rs` (`describe`) | unit tests per check in `custom/`; `crates/parvu-core/tests/custom.rs`: each check fails on a crafted body (file function, other table, `random()`, aggregate in a row function, wrong result type, failing example, cycle), and the three worked examples pass |
| **2. Apply row functions and operations** | `Operation::CustomFunction`, `CustomOperation`, `OpContext::custom`, `Workspace::set_custom`, use-time checks, `check_operation` branch | `ops.rs`, `workspace.rs`, `readonly.rs` | `ops.rs`: built SQL for in place / new column / filter / step / new table, and argument errors; `tests/workspace.rs`: apply, undo, `plan_result` → `materialize` → `add_result`, Refresh after the definition changed and after it was deleted; `tests/readonly.rs`: a custom call with a closed table is refused |
| **3. Store and commands** | `~/.ParVu/custom`, load, save, delete, import, export, revalidation on start, `custom-changed` event, `custom_value` job | `parvu-config/src/paths.rs`; new `parvu-app/src/custom_store.rs`, `custom.rs`; `commands.rs`, `main.rs`, `state.rs`; `ui/src/lib/api.ts` | store round trip, corrupt file skipped, newer `schema` left alone, stale `compiled` rebuilt, import with a name clash; `cargo test -p parvu-app` |
| **4. UI** | Manager, editor, apply dialog, menus, translations | new `ui/src/dialogs/CustomToolsDialog.tsx`, `CustomEditorDialog.tsx`, `CustomApplyDialog.tsx`; `dialogs/logic.ts`, `dialogs/lazy.tsx`, `App.tsx`, `lib/api.ts`, `components/Icon.tsx`; the three locale files | `dialogs/__tests__/custom.test.ts`: `buildCustomOp`, `customArgProblems`, signature text, column filtering by family; `cargo test -p parvu-config` for locale parity |
| **5. Expression language and formulas** | Custom functions in expressions, formulas, autocomplete; expression bodies | `parvu-dsl/src/{registry,compiler,resolver,lib}.rs`; `commands.rs` (registry accessor); `ui/src/lib/exprComplete.ts` | new golden cases with a custom registry in `crates/parvu-dsl/tests/golden.rs` (existing 306 unchanged); resolver type mismatch; `tests/formulas.rs`: a custom aggregate as a total; a formula whose function was deleted is kept with an error |
| **6. MCP and assistant** | `list_custom_tools`, `propose_custom_tool`, headless loading, docs | `mcp.rs`, `mcp_attach.rs`, `ai_assistant.rs`, `docs/MCP.md`, `docs/AI.md`, new `docs/CUSTOM.md` | `mcp.rs` tests: apply a custom operation through `apply_operation`; a proposal with `read_text` is refused; headless cannot define |
| **7. Later** (done except SQL-mode macros) | `columns` parameters everywhere, save steps as an operation, SQL-mode macros, custom aggregates in Group By (`reshape::AggFunc`) | `workspace.rs` (`Step`), `reshape.rs`, `GroupByDialog.tsx` | per feature |

Phases 1–4 give a usable feature (define with a SQL body, apply from menus). Phase 5 adds expression bodies and calls inside expressions.

Each phase keeps `cargo clippy --workspace --all-targets -- -D warnings` clean and needs no `bundled-duckdb` build.

### 11.2 Risks

| Risk | Mitigation |
|---|---|
| `json_serialize_sql` cannot round-trip some syntax, or prints SQL that binds differently | check 12 (stable compile); examples run on the **template**, not the source, so a wrong round trip fails an example |
| Slot replacement in text | `__pv_` is refused in bodies, so slots are unambiguous; covered by a test with `'__pv_arg_0'` in a literal |
| Probe types do not represent every type in a family (e.g. `HUGEINT`, `DECIMAL(38,10)`) | the use-time bind and result check (§6.8) catch the rest |
| An author's operation is slow or memory-hungry on a big table | warnings in check 10; jobs with cancel; `RESULT_ROW_LIMIT`; memguard. Not fully solvable for arbitrary SQL. |
| Definitions change under saved formulas and Refresh | documented behaviour; errors name the definition; formulas are kept, not dropped |
| DuckDB upgrade changes behaviour | revalidation with examples on start (§7.4) |
| A fresh in-memory database lacks the JSON functions in the prebuilt build | fall back to a cloned workspace connection with TEMP tables |
| Scope creep: the signature builder and example grids are the largest UI piece in the app | ship SQL bodies first; `columns` parameters and steps recording are phase 7 |

### 11.3 Alternatives rejected

| Alternative | Why not |
|---|---|
| DuckDB macros as the runtime | §5.5: no types, steps would depend on catalog state, the guard cannot see inside |
| Binding arguments as extra projected columns (`SELECT *, "Phone" AS p0 …`) instead of filling slots | no text filling at all, but constants become columns: `LIMIT n` and constant-only function arguments stop working, and constant folding is lost |
| Storing the syntax tree instead of SQL text | tied to DuckDB's JSON format across versions; unreadable in exports |
| A new dedicated language for operations | a third language beside SQL and expressions; SQL already does it |
| Python or another scripting runtime ("or something else") | ships an interpreter, breaks the read-only safety model, cannot be type-checked by DuckDB |
| Synthesising the body from examples | unreliable; no body to audit |
| All definitions in one JSON file | harder to share; one bad edit loses everything |
| Auto-rewriting whole-table aggregates in operation bodies | changes results on empty tables |

### 11.4 To verify against the shipped DuckDB before coding

1. `json_serialize_sql` / `json_deserialize_sql` round-trip `QUALIFY`, window clauses, `* REPLACE`, `* EXCLUDE`, CTEs; which statements they refuse.
2. `duckdb_functions()` has `stability` and `has_side_effects`.
3. A fresh `Connection::open_in_memory()` in the prebuilt build has the JSON functions.
4. `DESCRIBE` of a query over a `WHERE false` relation gives the same types as over real data (expected yes).
5. Error text for a non-grouped column beside an aggregate, used by the shape probe; prefer matching on the error class, not the wording.
6. Macro facts in §5.5, only if question 5 below is answered yes.

### 11.5 Open questions for the user

1. **Body languages.** Is "expression language for functions, SQL SELECT for operations" acceptable, or must operations be definable without writing SQL from the first version (which moves "save steps as an operation" from phase 7 to phase 2 and roughly doubles the work)?
2. **Are examples mandatory?** The design requires at least one passing example to save, with drafts as the escape. Stricter (require an example with an empty input) or looser (examples optional, with a warning)?
3. **Live or frozen in formulas and Refresh.** Formulas and refreshed result tabs follow the current definition; applied steps keep the SQL they were built with. Is that the behaviour you want, or should result tabs also freeze the definition they were made with?
4. **AI.** Propose-only with the user saving in the editor, as recommended, or may the assistant save definitions after an approval card? Should headless `parvu mcp --allow-write` be able to define?
5. **SQL mode.** Should custom functions also be callable in SQL mode (through DuckDB macros registered on the workspace)? It adds catalog state and a second code path.
6. **Output names.** In v1 an operation's new column names are fixed by its body (`AS rank`). Is a "name" parameter kind (the user types the new column's name) needed at the start?
