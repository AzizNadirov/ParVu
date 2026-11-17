# Operations by what they produce: Rows, Column, Table

Goal: the menus say what an operation does to your data. Today Data, Reshape and Operations mix three different kinds of work, and every one of them silently replaces the tab's view with a transformed result, after which cells and formulas stop working. The new rule:

| Kind | Produces | Where the result goes | Menu |
|---|---|---|---|
| **Row** operations | the same table, fewer or reordered rows | this tab, as a step (undo with Ctrl+Z) | **Rows** |
| **Column** operations | a new or changed column | this tab | **Column** |
| **Table** operations | a different table (grouped, pivoted, joined…) | **a new tab**; the source tab stays as it was | **Table** |

## 1. The menu bar

```
File   Edit   View   Rows   Column   Table   AI   Help
```

Data, Reshape and Operations go away; every item moves to exactly one of Rows, Column or Table.

```
Rows                              Column: qty ▸ (the selected column)      Table
─────────────────────────────     ───────────────────────────────────      ──────────────────────────────
Filter Rows…        Ctrl+Shift+L  New Formula Column…                      Group By…              Ctrl+G
Sort…                             ─────────────────                        Pivot Table…
First, Last or Random Rows…       Rename…                                  Unpivot (Columns to Rows)…
─────────────────────────────     Duplicate                                Transpose
Drop Duplicates…                  Remove                                   ──────────────────────────────
Drop Rows with Empty Values…      ─────────────────                        Value Counts…
                                  Read as ▸         (file columns)         Describe All Columns
                                  Change Type ▸ / Convert…                 ──────────────────────────────
                                  Text ▸ Extract ▸ Date ▸ JSON ▸           Join with Another Tab…
                                  Replace Values…                          Append Other Tabs…
                                  Fill Missing Values…
                                  ─────────────────
                                  Split Column…
                                  Merge Columns…
                                  Bin Values…
                                  Running Totals, Ranks and Rolling…
                                  ─────────────────
                                  Unique Values…   Statistics…   Chart…
```

- **Column** acts on the selected column. Its title names it (`Column: qty`) so it's clear what it will touch. With no column selected, its items are disabled except **New Formula Column…**.
- **Right-clicking a header** opens the same Column menu, plus Sort A→Z / Z→A, Filter by…, Freeze and Conditional Formatting. It is built from the same code, so the two menus can't drift apart.
- **Math Operation** is folded into **New Formula Column**: one way to compute a column, live, with the formula bar, autocomplete and Excel functions.
- **Fill** (down, up, right, left, Series…) moves to **Edit**. It edits cells; it's not an operation.
- The buttons above the table follow the same grouping: Filter (Rows), Formula (Column), Group by / Join / Append (Table), Chart.

## 2. Table operations open a new tab

```
┌ SQL Query (Table: orders_by_region)  [from orders · Group By ↻] ──────────┐
│ SELECT * FROM orders_by_region                                            │
...
[ orders ✕ ] [ orders_by_region ● ✕ ] [+]
```

- **Group By, Pivot, Unpivot, Transpose, Value Counts, Describe, Join and Append** open their result in a **new tab**, which becomes the active one. The source tab keeps its data, steps and edits.
- **The name** says what happened: `orders_by_region`, `orders_pivot`, `orders_unpivoted`, `orders_described`, `orders_join_customers`, `orders_append`. It can be renamed like any tab.
- **The result is a table of its own.** Cells can be edited, formula columns added, and any operation applied to it. This removes today's dead end, where a group by made everything read-only.
- **Where it came from:** a chip next to the table name, `from orders · Group By ↻`.
  - Clicking the chip switches to the source tab.
  - **↻ Refresh** re-runs the operation on the source's current data, for example after editing a cell there. If the result has unsaved edits, ParVu asks first.
- **Not a file yet:** the tab has the unsaved dot. `Ctrl+S` opens **Save As** (Parquet, CSV, JSON, Excel), and closing the tab asks first.
- **Undo:** the new tab starts with one step, "Created from orders: Group By region", which can't be undone. To discard the result, close the tab. Undo in the source tab doesn't touch the result.

### Memory and big results
- Results are **computed once and kept in memory**, so they stay fast and independent of the source.
- This runs as a background job with progress and **Stop**, like unique values today.
- Group by, pivot, value counts and describe are small by nature.
- If a result would pass **5,000,000 rows** (a join or append of big files, say), ParVu says so and offers to **save it straight to a Parquet file** and open that file, instead of filling memory.

### SQL stays as it is
Queries typed in the SQL editor still change the current tab, as a step. They're the power-user path. A query can produce a new table too: a **"Result in new tab"** toggle next to Execute Query, off by default.

## 3. Formulas start from columns, not cells

- Typing `=` in a cell **no longer starts a formula**. `=x` is just a value: text in a text column, rejected in a number column like any other non-number.
- **The first time** someone types `=` in a cell, the status bar explains: "Formulas are columns: use Column → New Formula Column, or the fx button."
- **Entry points for a formula column:**
  - **Column → New Formula Column…** inserts the column after the selected one;
  - the column header menu;
  - the **Formula** button above the table;
  - a **`+`** header after the last column (a pseudo-column, like a spreadsheet's next empty column), which starts a new formula at the end.

```
┌ order_id ┬ region ┬ qty ┬ ƒx total ┬  +  ┐
```

- **The formula bar is not part of the main window.** It opens above the grid only while a formula is written or changed (from the entry points above, or by opening a formula column's cell) and closes on apply or cancel.

## 4. Later: a blank table

**Table → New Empty Table** (and File → New): a 5×20 grid with Insert / Delete Rows and Add Column. Results of table operations (section 2) already give us tables in memory, so this builds on the same kind of tab. It's a phase of its own.

## 5. Phases

| Phase | Content |
|---|---|
| **U1. Menus and formula entry** | Rows / Column / Table menus; the header menu shares the Column menu; Math Operation folded into formulas; Fill under Edit; `=` in a cell is a value, with the hint; the `+` formula header. |
| **U2. Table operations in new tabs** | Results in memory, as a job with progress and Stop; tab naming; provenance chip with Refresh; Save As for tabs without a file; the 5M-row safeguard (save to Parquet); "Result in new tab" for SQL. |
| **U3. Blank tables** | New Empty Table; insert and delete rows; add columns. |

## 6. Status

U2 done: results in memory (`__pv_res_N` tables) computed as a job with progress and Stop; names after the operation; the provenance chip with Refresh; Save As for result tabs and a close warning; the 5M-row safeguard (write to Parquet or CSV); "Result in new tab" for SQL; AI table operations open a new table. Not done: renaming tabs (tables are referenced by name in other tabs' queries and formulas).

U1 done: the menus, the shared header menu, Fill under Edit, `=` in a cell as a value with a one-time hint, the formula bar shown only while editing, and the `+` header. Math Operation stays in the Column menu until U2.

## 7. Open questions

| Question | Proposal |
|---|---|
| Keep "transform in place" for Group By and the others? | No: one behaviour is easier to learn. SQL still transforms in place. |
| A Rows menu, or keep filter and sort in Data? | Rows: it says what they touch. |
| The memory limit for results | 5,000,000 rows, then offer Parquet. |
| The AI's `apply_operation` with a table operation | Same rule: the approval card says "opens a new tab"; the AI can then work on that tab by its name. |
