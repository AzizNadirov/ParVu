# Formulas and filling

Goal: Excel-like formulas and fill features in the grid. You type a formula in the expression language, references to columns are highlighted in the editor and in the grid, and you fill cells by dragging the fill handle or continuing a series.

The rule above all still holds: **fast with large files.** A formula is one SQL expression over every row, computed lazily for the rows a page shows. It never becomes millions of stored cell edits.

## 1. Decisions (made)

| Question | Decision |
|---|---|
| What is a formula? | A **column formula**, like an Excel Table's calculated column: `= [qty] * [price]` gives every row its value. |
| Live or a step? | **Live.** Formula columns are part of the table, next to the file and the cell edits. Editing `qty` updates `total` at once, cell editing stays allowed, and undo covers adding, changing and removing a formula. |
| Filling | **Fill handle drag**, **series / autofill**, **fill up / left / right** (next to the existing Fill Down). |

## 2. The model

### 2.1 Where formulas live

Today a tab is three relations:

```
__pv_src_sales      the file (typed: detection, Read as)
sales               __pv_src_sales + pending cell edits (a CASE overlay, edits.rs)
SELECT * FROM sales the tab's query; transforms wrap it as steps
```

Formulas become a fourth layer between the edits and the query:

```
__pv_src_sales  →  __pv_edits_sales (cell edits)  →  sales (+ formula columns)  →  steps
```

- `sales` becomes `SELECT *, <f1> AS "total", <f2> AS "margin" FROM __pv_edits_sales`, with formulas in dependency order: a formula may use columns made by earlier formulas.
- The tab's query (`SELECT * FROM sales`) does not change. So adding a formula is **not a transform**, and cell editing stays allowed, the same trick "Read as" uses (`StepKind::SourceReading`).
- Each change is recorded as `StepKind::Formula { name, previous: Option<Formula> }`, so undo restores the previous formula or removes the column.
- Edits to the source columns change the edits layer, and the formulas recompute on the next page. That is what makes them live.
- Formula columns are **read-only for cell edits**. Typing into one opens its formula in the formula bar instead.
- Steps added later (filters, group by, …) see formula columns as ordinary columns.
- Saving or exporting writes the computed values; the file has no place for formulas.

### 2.2 Formula

```rust
pub struct Formula {
    pub name: String,      // the column
    pub source: String,    // what the user typed: "[qty] * [price]"
    pub sql: String,       // compiled by parvu-dsl (in the app; core stays DSL-free)
    pub after: Option<String>, // shown right after this column (default: at the end)
}
```

`parvu-core` never depends on `parvu-dsl` (AGENTS.md). The app compiles `source` and passes `sql` down, the way `AddColumn` already receives compiled expressions. The core validates the result with `LIMIT 0`, then counts rows, which costs nothing because no formula is evaluated by a count.

### 2.3 The language

Formulas use the expression language as an expression, without the `t[x] =` assignment:

| Formula | Meaning | SQL |
|---|---|---|
| `= [qty] * [price]` | this row's values | `"qty" * "price"` |
| `= sales[qty] * 2` | the full form still works | same |
| `= IF([qty] > 10, 'bulk', 'single')` | any function | `CASE …` |
| `= SUM([price])` | the column's total *(phase F4)* | a cached constant, see §4 |
| `= [price] / SUM([price])` | share of the total *(F4)* | value / cached total |
| `= SUM([q1:q4])` | a row-wise range of columns *(F4)* | `"q1" + "q2" + "q3" + "q4"`, NULLs as 0 |
| `= PREV([total])`, `NEXT([x], 2)` | neighbouring rows *(F4)* | `LAG` / `LEAD` over the file's row order |

New in the language:
- **`[col]`** for the current table, no table name needed. This needs a small parser change and new golden cases.
- **Ranges `[a:c]`** of adjacent columns, in table order, for row-wise `SUM`, `AVG`, `MIN`, `MAX`, `COUNT`. A single column inside these functions keeps meaning the column total, as in Excel (`SUM(B:B)` versus `SUM(B2:D2)`).

## 3. Editing formulas

### 3.1 Formula bar

A bar above the grid, like Excel's *fx* bar:
- **Selected cell in an ordinary column:** shows the cell's value, editable the way the grid edits it today.
- **Selected cell in a formula column:** shows `= <source>`. Editing it changes the column's formula.
- **Typing `=` starts a formula:**
  - in the formula bar, or in a cell of the empty column after the last one, it creates a formula column (name asked, default `column1`…);
  - in a cell of an ordinary column, it offers "Make *qty* a formula column?", which replaces its values (P1).
- **The editor** is CodeMirror with the expression language already in `QueryEditor.tsx`: syntax highlighting, autocomplete of functions and columns, bracket matching.
- **Keys:** Enter applies, Esc cancels, Shift+Enter adds a line.
- **Errors** from the compiler are underlined at their position (DSL errors carry line and column) and explained under the bar. Nothing changes until the formula is valid.

### 3.2 Range highlights (Excel's coloured references)

While a formula is being edited:
- each referenced column gets a colour from the chart palette (the colour-blind-checked one, in light and dark versions);
- the reference token in the editor is drawn in that colour;
- the referenced columns in the grid get a coloured header border and a light tint of the same colour;
- clicking a column header or a cell inserts `[column]` at the cursor; Shift+click on a second header inserts the range `[a:c]`;
- the formula column itself is marked with an *fx* header icon (a new sprite, next to the type icons).

### 3.3 Where else formulas appear

- Column menu: **Edit formula…**, **Convert to values** (the computed values become an ordinary column, through `AddColumn` as a step), **Remove formula**.
- Table Info: lists formula columns with their source.
- Optionally, the formulas of a file are remembered next to its "Read as" choices in `column_types.json`, applied when it reopens while unchanged.

## 4. Speed

| Formula | Cost |
|---|---|
| Row expressions (`[a] * [b]`, functions) | Lazy: computed only for the rows on screen, and skipped by row counts. |
| Column totals (`SUM([price])`) | Computed **once**, when the formula is applied and again after a cell edit, by one scan of that column. Baked into the SQL as a constant, so pages don't rescan. |
| `PREV` / `NEXT` | A window over the whole table on every page. Allowed, with a note in the formula bar that it reads the whole file each time. |
| Ranges `[a:c]` | Row expressions, lazy. |

## 5. Filling

All fills are **cell edits**: they need an editable tab (no transform applied, as today), skip formula columns, and are capped at 100,000 cells per action (`edit_cells`). They work on what is loaded, a page of up to 10,000 rows. A whole column is a formula's job (e.g. `= ROW_NUMBER()`, P1).

| Feature | How |
|---|---|
| **Fill handle** | Glide's built-in handle (`fillHandle`, `allowedFillDirections`, `onFillPattern`, v6.0.3). ParVu cancels Glide's plain copy in `onFillPattern`, computes the values from the source rectangle with the series rules, and saves them as one edit step. |
| **Series rules** (pure TypeScript, unit-tested) | numbers: the step from the seed cells (1, 2 → 3, 4…; 5, 10 → 15; one seed copies, like Excel); dates and timestamps: by day, week, month or year from the seeds; month and day names in en, ru, az (Jan, Feb…; Mon, Tue…); text ending in a number (`Item 1` → `Item 2`); anything else repeats the seeds in order. |
| **Fill down / up / left / right** | Data menu and cell menu: copy the selection's edge row or column over the rest. Ctrl+D (down, exists) and Ctrl+R (right, as in Excel). |
| **Series… dialog** (P1) | Start, step, stop and unit for a selected range, for when dragging is awkward. |

## 6. Phases

| Phase | Content | Size |
|---|---|---|
| **F1. Formula layer** (done, #44) | `Formula`, the edits view split from the formula view, dependency order and cycle check, `StepKind::Formula`, read-only formula cells, values in save and export, `[col]` shorthand in the language; core and app commands; tests on Parquet and CSV (live recompute after a cell edit, undo, a step after a formula, a large file stays lazy). | M |
| **F2. Formula bar** (done) | The bar with CodeMirror highlighting and autocomplete, `=` in a cell or the *fx* button to start, errors under the bar, coloured references in editor and grid, click a header to insert a reference, *fx* icon, Edit / Remove / New formula column in the column menu. Moved to F4: Shift+click ranges (with `[a:c]`), Convert to values, formulas in Table Info, error positions underlined. | M |
| **F3. Filling** (done) | Fill handle with series rules (numbers, dates by day / month / year and month ends, timestamps, month and day names in en / ru / az, text ending in a number, codes with leading zeros), fill down / up / right / left in the Data and cell menus, Ctrl+R; tests for every rule (`ui/src/lib/fill.ts`). | M |
| **F4a. Formula language** (done) | Cached column totals (markers in the compiled SQL, filled in by `formula::with_totals` on every view rebuild), ranges `[a:c]` (row-wise SUM / AVG / MIN / MAX / COUNT), `PREV` / `NEXT` (`LAG` / `LEAD OVER ()`, capped at 2,000,000 rows), Shift+click ranges, error positions underlined, formulas in Table Info. Dropped: *Convert to values* (saving already writes values; a copy as a step would block cell edits). | M |
| **F4b. Formulas elsewhere** (done) | `set_formula` MCP tool (approval card with a preview; also the built-in assistant), formulas remembered per file in `column_types.json` (sources, recompiled on open, while the file is unchanged), Series… dialog (linear, growth, date units, stop value). | M |

Each phase is its own PR, with docs (`docs/DSL.md`, Quick Start) and changelog entries.

## 7. Open questions

| Question | Recommendation |
|---|---|
| Typing `=` in a cell of an ordinary column | Ask before turning the column into a formula; never silently. |
| A cell edit in a formula column | Not allowed; open the formula. Excel's "this cell differs from the column formula" exception is not worth the complexity. |
| Remember formulas per file | Yes, with the "Read as" choices, until the file changes. |
| `PREV` / `NEXT` on huge files | Allowed with the note, rather than refused. |
