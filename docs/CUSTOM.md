# Custom functions and custom operations

ParVu lets you add your own tools to the **Column**, **Rows** and **Table** menus:

- a **custom function** takes columns (and values) and gives a column, or one value for the whole column;
- a **custom operation** takes a table (and columns and values) and gives a table: it either changes the table as an undoable step, or opens its result as a new table, like Group By.

You write the body in SQL (DuckDB's dialect) or, for a function, in ParVu's expression language, declare typed parameters, and give **examples**: small inputs with the output you expect. ParVu checks everything before saving, and runs the examples as tests. A tool is offered in the menus only once every check passes.

ParVu comes with nine samples you can use, change, copy or delete: `clean_phone`, `title_case`, `percent_of_total`, `weighted_average`, `percentile_value`, `top_n_per_group`, `outliers_iqr`, `running_total` and `dedupe_by`. A deleted sample stays deleted; **Restore samples** in the manager brings back the ones that are missing.

## Using one

- **Column menu → Custom functions** (also in the column header's right-click menu) lists the functions whose first column parameter accepts the selected column's type. A row function asks for its arguments and writes into the same column or a new one right after it, optionally only on rows that match a condition. A summary function (one value) shows the value with a Copy button; nothing in the table changes.
- **Rows menu** lists the operations that change the table. Each run is one step in Applied Steps; Undo removes it.
- **Table menu** lists the operations that open a new table, then **New custom operation…** and **Manage custom tools…**.

The apply dialog is made from the signature: a column picker per column parameter (only columns of a fitting type), a checklist per columns parameter (the columns that fit, with how many it needs), a typed box per value, a table picker per extra table. A preview shows the first rows of the result.

### In Group By

A summary function with exactly one column parameter (the others values), such as `percentile_value`, appears in Group By's list of calculations under **Custom functions**. Pick the column it aggregates; boxes for its values appear next to it (empty: the default). It runs once per group. Pivot and charts don't offer them.

Drafts and tools that need attention appear in the menus greyed out.

### In expressions and formulas

A usable custom function can also be called like a built-in one:

- in the expression editor: `contacts[mobile] = clean_phone(contacts[Phone])`;
- in a formula column: `= clean_phone([Phone], "00")`;
- a **summary function** in a formula is a column total, like `SUM`: `= [price] - weighted_average([price], [qty])` computes the weighted average once and subtracts it from each row.

Autocomplete lists custom functions beside the built-ins (marked *custom*, and *total* for a summary function), with their typed signature and description. Arguments are checked against the signature: `clean_phone([qty])` is refused with "clean_phone: phone takes text, but this argument is a number" when the type of the argument is known. Optional value parameters may be left out and take their default; a function takes no more arguments than its signature has.

**Formulas follow the current definition.** When a custom function changes, the formulas that call it are compiled again. When it is deleted or needs attention, the formula is **kept**: its column is empty and shows a warning icon in the header, and opening the formula shows why. Fix the formula, or bring the function back (Restore samples for a sample), and the column fills again. The same happens when a file opens with a remembered formula that no longer compiles.

Operations are not callable from expressions.

### In SQL mode

A usable custom function can be called by name in SQL mode, as if DuckDB had it:

```sql
SELECT clean_phone(Phone) AS phone, clean_phone(Phone, '00') AS intl FROM contacts WHERE clean_phone(Phone) IS NOT NULL;
SELECT region, weighted_average(price, qty) FROM sales GROUP BY region;
SELECT *, weighted_average(price, qty) OVER (PARTITION BY region) AS region_avg FROM sales;
```

- **How it works.** Before the query runs, ParVu replaces each call with the function's compiled SQL, the arguments put in place of its parameters. DuckDB never sees the name, so nothing is registered in the database. The step in Applied Steps keeps that **expanded** SQL (its label shows what you typed), like every other step: changing or deleting the function later doesn't change it, and Undo works as usual. The SQL editor keeps what you typed. **SQL to new tab** expands the same way, and Refresh of such a tab uses the functions' current definitions.
- **Arguments.** A column parameter takes any expression (`clean_phone(lower(Phone))`, `clean_phone(a || b)`, another custom call). A value parameter takes any expression too, cast to its declared type. Optional values may be left out and take their default. Arguments can be named: `clean_phone(prefix := '00', phone := Phone)`. A wrong number of arguments is refused with the signature, e.g. "clean_phone takes 1 to 2 arguments: clean_phone(phone: column<text>, prefix: value<VARCHAR> = "+") → column<text>".
- **Summary functions** (one value) are aggregates: over the whole table, per group with `GROUP BY`, or per window with `OVER (…)`. A summary function's SQL may hold several aggregates (`weighted_average` is `sum(x * w) / sum(…)`); `OVER (…)` and `FILTER (WHERE …)` on the call are given to **each** of them, so `weighted_average(price, qty) OVER (PARTITION BY region)` is `sum(…) OVER (PARTITION BY region) / sum(…) OVER (PARTITION BY region)`. `DISTINCT` or `ORDER BY` inside the call is refused, and so is `FILTER` when the function's own SQL already filters an aggregate.
- **Types.** Column types aren't checked before the query runs (an argument can be any expression); DuckDB checks them. When its error comes from inside a custom function, the message starts with the function's name and signature, e.g. "clean_phone: an argument has a type this function doesn't take. It is clean_phone(phone: column<text>, …)".
- **Not callable in SQL mode:** functions with a list-of-columns parameter (`columns`), custom operations (use them from the menus), and `OVER` on a row function. Each is refused with a message naming the function. Drafts and tools that need attention are refused too.
- **DuckDB wins.** A name DuckDB itself has is never expanded. (Saving refuses such names, but a later DuckDB may add one.)
- A name inside a string (`'clean_phone'`) or a quoted column name (`"clean_phone"`) is left alone, and a query that calls no custom function runs exactly as typed.
- An unaliased call keeps the column name DuckDB would give it: `SELECT clean_phone(Phone)` gives a column named `clean_phone(Phone)`.
- Autocomplete in the SQL editor lists the callable custom functions (marked *custom function* or *custom summary*), with their signature and description.

## Defining one

**Table → Manage custom tools… → New function** (or **New operation**) opens the editor.

1. **Name**: `a-z`, `0-9` and `_`, starting with a letter, at most 40 characters. It may not be a DuckDB function, a SQL keyword, a function of the expression language or another custom tool. **Title** and **Description** are what people see in menus and dialogs.
2. **Signature**: one row per parameter.

   | Kind | Means | Type |
   |---|---|---|
   | columns | several columns of the table, each of a type in the set; at least `min` (1 by default), at most `max` | the same type sets as column |
   | column | one column of the table | a set of types: `any`, `number`, `integer`, `decimal`, `float`, `text`, `boolean`, `date`, `datetime`, `date_or_datetime`, `time`, `interval`, `json`, `list`, `struct` |
   | value | one constant typed in the dialog | a DuckDB type: `VARCHAR`, `BIGINT`, `INTEGER`, `DOUBLE`, `DECIMAL(18,3)`, `BOOLEAN`, `DATE`, `TIMESTAMP`, `TIME`, `INTERVAL` |
   | table | operations only: the first is the table the operation runs on; more tables are chosen from the open tabs | optional required columns, e.g. `amount: number, region` |

   Value parameters may be **optional**, with a default. Optional parameters come last. SQL keywords such as `group`, `order` or `by` can't be parameter names.

   The **result** of a function is **a column** (one value per row) or **one value** (an aggregate such as `SUM`), with a type, or "same as" a column parameter. An operation either **changes this table** or **opens a new table**; with more than one table parameter it always opens a new table.
3. **Body**
   - A function's body is one SQL expression over its parameters, written by name: `upper(name) || '!'`, `x / sum(x) OVER ()`, `sum(x * w) / sum(w)`.
   - Or, with **Language: Expression language**, one expression in ParVu's expression language (the one formulas use), with `[x]` for parameter `x`: `ROUND([x] * 3)`, `SUM([x] * [w]) / SUM([w])`. Typing `[` offers the parameters. ParVu translates it to SQL, which then takes exactly the same checks; **Compiled SQL** shows the result. An expression body can use the built-in functions but not other custom functions, XLOOKUP or VLOOKUP. Operations are always written in SQL.
   - An operation's body is one `SELECT` that reads its table parameters by name (`FROM t`) and uses column and value parameters by name: `SELECT * FROM t WHERE amount > limit_value`.
   - A **columns** parameter stands for its columns written one after another (`"region", "city"`). It may appear only on its own (no table name, no alias, no arithmetic) where SQL takes such a list: an item of the select list (operations), `GROUP BY`, `PARTITION BY`, `DISTINCT ON`, `ORDER BY` without `ASC`/`DESC`/`NULLS` (a direction would apply to the last column only), and the arguments of a function or `COALESCE`: `SELECT keys, count(*) AS n FROM t GROUP BY keys`, `concat_ws(' ', parts)`. Anywhere else validation refuses it ("… is a list of columns"). It is probed with two columns of each accepted type. Functions with a columns parameter can't be called in expressions or formulas (yet); use them from the Column menu.
   - Ctrl+Enter validates.
4. **Examples**: at least one is required. A function's example is a small grid with one column per column parameter, the value arguments, and the expected output next to each row (or one expected value for a summary function). An operation's example is an input table, the column chosen for each column parameter, the value arguments, and the expected table. Tables are compared as a set of rows unless **Compare rows in order** is ticked; numbers match to 9 significant digits. An empty cell means "no value"; type `""` for empty text.

   **Fill expected from result** runs the body on the example's input and puts its output in Expected. Read it before saving: that's the point of examples.

**Validate** runs every check and lists the results. **Save** is enabled only when the last validation of what you see passed (and the save validates again). **Save draft** keeps an unfinished or failing definition; drafts are not offered in menus until they pass.

## What the checks do

Validation runs on a small in-memory DuckDB of its own (256 MB, two threads, examples stopped after 5 seconds). It never sees your open tables.

- **Safety.** The body must be one `SELECT` (or one expression). No file functions (`read_csv`, `read_text`, `glob`…), no file paths as tables, no other tables than the table parameters, no `getenv` or `current_setting`, no DDL or DML. A function body may not read any table or use a subquery. The text `__pv_` is reserved.
- **Repeatable.** Functions that give a different result each time (`random()`, `uuid()`, `nextval()`…) are refused. Functions of today's date or the current time (`now()`, `current_date`, `today()`…) are allowed only when **Depends on today's date** is ticked; such a tool shows a badge.
- **Types.** The body is bound against empty tables of each type the parameters accept (BIGINT, DECIMAL and DOUBLE for `number`, and so on; `any` tries BIGINT, DOUBLE, VARCHAR, DATE and BOOLEAN), and the result's type must fit the declaration.
- **Shape.** A row function must not aggregate; a summary function must.
- **Examples.** Every example must give the expected output.
- **Warnings** (they don't stop saving): an unused parameter; `LIMIT` without `ORDER BY`, `row_number()` or `string_agg` without an order (the result depends on row order); a whole-table `median`, `mode`, `quantile` or `count(DISTINCT …)` in an operation (may need a lot of memory on a big table: group by a constant, as `outliers_iqr` does, to let DuckDB spill to disk); no example with an empty cell.

When you apply a tool, ParVu checks again on the real table: the columns exist and have fitting types, values parse and respect their choices and bounds, and the result has the declared type.

Ties in an `ORDER BY` can't be found by the checks: `row_number()` splits equal values arbitrarily, `rank()` keeps them.

## Save steps as a custom operation

**Save as operation…** in the Applied Steps panel (or **Table → Save steps as custom operation…**) turns a tab's steps into a custom operation:

- Each step is listed: ✓ if it is part of the operation, otherwise why not. Cell edits, formulas and "Read as" choices change the table the steps start from, so they are not repeated. A step that reads another tab (a SQL query naming it) is left out, with every step after it.
- The table under the steps becomes the table parameter `t`. Each step becomes a `WITH step_N AS (…)` block reading the one before; SQL typed in SQL mode reads `t` where it named the tab.
- **Columns the steps use** become required columns of `t` (same name, a fitting type), or **parameters** where you tick them: then you choose the column when you apply the operation. A column a step writes (a new or replaced column of that name) stays fixed.
- The operation opens in the editor, prefilled, with one example: the tab's first rows (up to 10, or 50 when the steps keep none of the first 10) and what the steps make of them. Read it, Validate, Save.

Limits: many steps list the table's columns one by one (a new column placed after another, a column function in place), so those columns become required and other columns of the target table are dropped. Values a step worked out from this table when it ran (the number of parts of Split, bin edges) stay as they were. Steps applied before ParVu recorded how they were made can't be saved.

## Steps, Refresh and changes

- A tool is compiled once, on save, into SQL in which the arguments are filled in as quoted names and typed literals.
- **Applied steps keep the SQL they were built with.** Changing or deleting a tool doesn't change steps already applied, and Undo still works.
- **Refresh** on a new table made by a custom operation uses the tool's **current** definition. If the tool was deleted, Refresh fails with "Custom operation … no longer exists" and the table keeps its rows.
- After an update of ParVu (or its DuckDB), every tool is validated again in the background, examples included. A tool that no longer passes is marked **Needs attention**, is not offered in menus, and ParVu says so once after start. Its file is not changed otherwise.

## Sharing: import and export

In the manager, **Export…** writes the ticked tools (or the selected one) to a `.json` file. **Import…** reads such a file. Each imported tool is validated as if typed in; one that fails is kept as a draft with its report, so you can open it and see why. When a name exists, ParVu asks: **Replace**, **Keep both** (the imported one gets a free name such as `clean_phone_2`) or **Skip**.

Importing is safe: a tool can't read files or other tables (see Safety), and its compiled form is never taken from the file.

### File format

One file holds one definition or several. ParVu writes a bundle:

```json
{
  "schema": 1,
  "definitions": [ { …a definition… }, … ]
}
```

and reads either a bundle or a single definition object. `schema` is the format version: a file with a higher number than this ParVu knows is refused on import, and listed but left untouched in the folder.

A definition:

```json
{
  "schema": 1,
  "kind": "function",
  "name": "clean_phone",
  "title": "Clean phone number",
  "description": "Keeps the digits of a phone number and puts a prefix in front.",
  "author": "ParVu",
  "params": [
    { "name": "phone", "kind": "column", "type": ["text"] },
    { "name": "prefix", "kind": "value", "dtype": "VARCHAR", "optional": true, "default": "+" }
  ],
  "result": { "kind": "column", "type": ["text"] },
  "body": {
    "language": "sql",
    "source": "CASE WHEN length(regexp_replace(phone, '[^0-9]', '', 'g')) < 7 THEN NULL ELSE prefix || regexp_replace(phone, '[^0-9]', '', 'g') END"
  },
  "flags": { "uses_current_time": false },
  "examples": [
    {
      "name": "typical",
      "input": { "columns": [{ "name": "phone", "dtype": "VARCHAR" }], "rows": [["+1 (555) 010-2030"], ["n/a"], [null]] },
      "args": { "prefix": "+" },
      "expected": { "columns": [{ "name": "result" }], "rows": [["+15550102030"], [null], [null]] }
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `kind` | `function` or `operation` |
| `params[].kind` | `column`, `columns`, `value` or `table` |
| `params[].type` | column parameters: type set names (empty: any) |
| `params[].dtype` | value parameters: the DuckDB type |
| `params[].optional`, `default` | value parameters that may be left out |
| `params[].choices`, `min`, `max` | value parameters: allowed values and number bounds; `min`, `max` of a columns parameter: how many columns |
| `params[].of` | column parameters of an operation: the table parameter they belong to (default: the first) |
| `params[].columns` | table parameters: required columns, `[{ "name", "type" }]` |
| `params[].description` | shown as a tooltip in the apply dialog |
| `result.kind` | `column` or `value` (functions), `table` (operations) |
| `result.type`, `result.same_as` | the result's type set, or the name of a column parameter whose type it has |
| `result.columns` | operations: columns the result must have |
| `placement` | operations: `step` (changes this table, default) or `new_table` |
| `body.language` | `sql`, or `expression` (functions only: ParVu's expression language, `[x]` for parameter `x`) |
| `flags.uses_current_time` | the body may use today's date |
| `examples[].input` | a function: one column per column parameter, named after it; an operation: its first table |
| `examples[].tables` | an operation's other table parameters, by name |
| `examples[].args` | value arguments, and for operations the column chosen for each column parameter; a columns parameter's columns as a list, `["store", "item"]` (functions too) |
| `examples[].expected` | the expected output; a function's has one column |
| `examples[].ordered` | compare table rows in order |

Cells are text or `null`, cast to the column's `dtype`. Files in the folder also carry `status` (`ok`, `draft` or `broken`), `created`, `modified`, `sample`, and two blocks ParVu computes itself: `compiled` (the SQL template, its hash and the ParVu and DuckDB versions it was made with; ignored on import, and rebuilt from the body each time ParVu starts, so editing it by hand has no effect) and `report` (the last validation of a draft or broken tool).

## Where they are stored

`~/.ParVu/custom/<name>.json`, one file per tool. A file that can't be read is listed in the manager as "Could not be read" and skipped. `~/.ParVu/custom/.samples_seeded` records that the samples were added; delete it to have them added again on the next start.

The headless MCP server (`parvu mcp`) loads the same folder, so `apply_operation` can run a custom operation or function there too; in the window, AI clients use the window's tools.

## AI clients and the assistant

- `apply_operation` takes a columns argument as `{"columns": ["store", "item"]}`, and a summary function of one column as a Group By aggregation: `{"column": "x", "func": "custom", "custom": {"name": "percentile_value", "values": {"p": "0.5"}}}`.
- `list_custom_tools` gives an AI client (and the built-in assistant) each usable tool: name, kind, typed signature, description, one example, and how to call it (`apply_operation` with `custom_function` / `custom_operation`, `set_formula`, or by name in `query`).
- `query` expands custom function calls the same way as SQL mode, **before** the read-only check, so the check sees the SQL that actually runs: a file function inside an argument (`clean_phone((SELECT content FROM read_text('/etc/passwd')))`) is still refused.
- `propose_custom_tool` lets the AI suggest a new tool: a full definition with examples. ParVu runs every check (the same safety guard refuses file functions and other tables) and returns the report, so the AI can correct it. In the window, ParVu then asks whether to open the proposal in the editor, prefilled, with a note saying who proposed it. **Nothing is saved unless you save it there.** The headless `parvu mcp` only validates proposals, even with `--allow-write`. See [MCP.md](MCP.md).

## The samples

| Name | Kind | What it does |
|---|---|---|
| `clean_phone(phone, prefix = "+")` | function → column | Keeps the digits of a phone number with a prefix; fewer than 7 digits become empty |
| `title_case(text)` | function → column | `aNNA de LIMA` → `Anna De Lima` |
| `percent_of_total(x)` | function → column | each value as a percentage of the column's total (a window: reads every row) |
| `weighted_average(x, w)` | function → one value | the average of `x` weighted by `w`, leaving out rows where either is empty |
| `top_n_per_group(t, grp, score, n = 3)` | operation, changes the table | keeps the `n` rows with the highest score in each group |
| `outliers_iqr(t, x, k = 1.5)` | operation, new table | the rows outside Q1 − k·IQR … Q3 + k·IQR (Tukey's fences) |
| `running_total(t, amount, sorted_by)` | operation, changes the table | adds `running_total`, the sum of `amount` up to each row in `sorted_by` order |
| `dedupe_by(t, keys: columns, latest)` | operation, changes the table | keeps one row per combination of `keys`: the one with the highest `latest` |
| `percentile_value(x, p = 0.9)` | function → one value | the `p` quantile of `x`, interpolated (0.5 is the median); also a Group By calculation |

## Not in this version

- Calling one custom function from another's body.
- Calling a function with a list-of-columns parameter, or a custom operation, in SQL mode.
- Passing a list of columns to a custom function in an expression or formula.
- A parameter for the name of a new column (a row function's output column is chosen in the dialog).
