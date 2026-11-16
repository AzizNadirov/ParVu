# Column types: detection, conversions and type-aware operations

Goal: ParVu knows what each column is (number, text, date, boolean, JSON…), gets it right when a file opens, converts types without ever breaking the table, and offers the operations that fit each type, in the spirit of Power Query's Transform / Add Column tabs.

The rule above all: **fast with large files.** Every operation stays a lazy SQL expression over the tab's query. Nothing scans a whole multi-GB file when a file opens or a menu opens; full scans happen only when the user asks, in the background, and can be cancelled.

Measured on DuckDB 2.0.0 (the prebuilt library; crate `1.20000.1`). Benchmarks ran on a 2M-row, 78 MB CSV, 2 threads, 1 GB memory limit.

## 0. What is wrong today

| Problem | Where | Effect |
|---|---|---|
| A bad value deep in a CSV breaks the tab | `format.rs:70` `read_csv(path)` | DuckDB guesses types from the first rows. A value like `oops` at row 300,000 in a number column makes any sort, sum, last page or export fail with `Conversion Error`. `ignore_errors` silently drops the row (data loss); `sample_size=-1` reads the whole file just to guess. |
| Change type breaks later | `ops.rs:341` plain `CAST` | The step is accepted (page 1 and the row count never touch the bad value), then paging to it, sorting or filtering on the column fails. |
| Export can stop halfway | `export.rs:173-179` plain `CAST` | A COPY aborts and leaves a partial file. |
| Ambiguous dates | DuckDB sniffer | `03/04/2024` is read day-first (3 April) with no hint; US files come out wrong. |
| Split column scans everything | `clean.rs:404` `MAX(LEN(split))` | With no part count it reads the whole result before showing anything. |
| Time zones in installers | `parvu-core/Cargo.toml:13` | `bundled` has `json` and `parquet` but not `icu`, so `AT TIME ZONE` works in development and probably fails in shipped builds. |
| No type knowledge in the UI | `ui/src/lib/chart.ts:122`, `page.rs` `is_numeric()` | The UI guesses types by regex on the dtype string; menus are the same for every column. |

## 1. Principles

1. **The table never crashes on data.** Every type conversion is `TRY_CAST` (or `try_strptime` and friends). A value that doesn't convert becomes NULL and is counted as an error, which the user can see, filter and fix.
2. **Opening stays instant.** Type detection reads a sample of at most 10,000 rows taken with `LIMIT` (pushed down: only the first row groups or lines are read). Never `USING SAMPLE` (that is a full scan) and never `sample_size=-1`.
3. **Per-row work only.** Text, date, boolean and JSON operations are column expressions. DuckDB drops unused ones from row counts and evaluates the rest only for the rows a page or export needs.
4. **Exact counts on demand.** Error counts and distinct-key scans run first on the sample (instant) and then, if wanted, over everything in the background via the existing `InterruptHandle` pattern of `ReadSession`.
5. **Decisions are stored in the step.** JSON keys, date formats and word lists chosen from a sample are saved in the `Operation`, so replaying a step list gives the same columns.

## 2. Type families

Add `TypeFamily` to `parvu-core` (`page.rs`) and send it with every `ColumnMeta`, so neither the UI nor the MCP tools parse dtype strings:

`Integer · Decimal · Float · Text · Boolean · Date · Timestamp · TimestampTz · Time · Interval · Json · List · Struct · Map · Binary · Other`

The UI gets helpers (`isTemporal`, `isText`, `isNested`…) in `ui/src/lib/types.ts` and shows a small type badge in every column header (`123`, `1.2`, `ABC`, `✓✗`, a calendar, a clock, `{}`, `[ ]`), like Power Query. Clicking the badge opens the type menu (§4).

## 3. Type detection when a file opens

### 3.1 Approach: DuckDB's typed reader, text only where needed, healing

*Revised after building it (phase 1a).* A first benchmark suggested reading every column as text and converting with `TRY_CAST` costs nothing. On a realistic file (2M rows / 98 MB: ids, amounts, dates, names, yes/no) it costs a lot, because DuckDB's typed reader parses straight into typed values:

| 2M rows / 98 MB | DuckDB typed read | All text + `TRY_CAST` | … with `trim()` |
|---|---|---|---|
| Count rows | 199 ms | 282 ms | 357 ms |
| Sum a column | 207 ms | 332 ms | 473 ms |
| Sort, first page | 347 ms | 534 ms | 942 ms |

So for CSV:

1. DuckDB sniffs the file (`DESCRIBE SELECT * FROM read_csv(...)`). ParVu's detectors (§3.2) run on the first 10,000 rows read as text.
2. Where they agree, the column stays **native**: DuckDB's type at full speed.
3. Where ParVu reads more (numbers like `1.234,5`, yes/no words DuckDB doesn't know, a second date format in the same file, an ambiguous date order, JSON), only that column is read as text (`read_csv(path, types = {'col': 'VARCHAR'})`) and converted (`TRY_CAST`, `try_strptime`, a `CASE`). No `trim()`: it costs as much as the conversion, and `TRY_CAST` already ignores spaces around numbers.
4. **Healing.** A native column can still meet a value it can't parse past DuckDB's 20,480-row sniff. DuckDB then fails the query with `Error when converting column "qty"` and `file = …`. `Workspace::heal` switches just that column to text + `TRY_CAST` and rebuilds the source view; the caller retries:
   - the UI repeats any command that returns error kind `healed`;
   - MCP tools retry through `ParvuMcp::read`;
   - background exports run again.

   The column keeps its type, and the bad value becomes NULL.
5. **DuckDB bug.** In DuckDB 2.0, the `type_pushdown` optimizer moves a `TRY_CAST` over a CSV text column into the CSV reader, which then parses strictly and fails. Every workspace connection runs `SET disabled_optimizers = 'type_pushdown'`. It is worth reporting upstream.

Measured with `cargo test -p parvu-core --test types -- --ignored --nocapture open_speed` (same file, 2 threads):
- **Opening** takes 495 ms against 217 ms for DuckDB alone. Detection is about 180 ms of that, and it reads only the sample, so it does not grow with the file.
- **Sort** (validate, count, first page) takes 655 ms against 456 ms. The difference is converting a `dd.mm.yyyy` column that DuckDB leaves as text.

Parquet keeps its stored types (no detection, nothing to break). A Parquet column stored as text can still go through the detectors on request ("Detect types" in the column and table menus).

Open item: JSON files. `read_json` also guesses from a sample (`sample_size` 20,480). Check whether `columns={...: 'VARCHAR'}` plus the same `TRY_CAST` layer is as cheap. Excel already infers per-column types from every cell in `load_excel` (`format.rs:191`); add the detectors for its text columns.

### 3.2 Detectors (on the 10,000-row sample)

A column gets a type only if **every** non-empty sample value parses. Otherwise it stays text. Later failures become NULL plus an error count, not a crash.

| Detector | Rule | Result |
|---|---|---|
| Identifier guard | all digits with a leading zero (`00123`), or longer than 18 digits | stays text (DuckDB already does this; keep it) |
| Integer / decimal | `TRY_CAST` succeeds | `BIGINT` / `DOUBLE` (or `DECIMAL(p,s)` if every value has the same number of decimals) |
| Locale number | matches `1.234,56` or `1 234,56` | `DOUBLE` via `replace` of the separators; the separators are stored in the column's parse options |
| Currency / percent | leading or trailing `$ € £ ₼ ₽ %` around a number | number (percent ÷ 100), symbol stored |
| Boolean words | values within {true,false}, {yes,no}, {y,n}, {1,0}, {t,f}, {on,off}, plus ru/az words | `BOOLEAN` via `CASE` |
| Date / timestamp | try ISO, `%d.%m.%Y`, `%d/%m/%Y`, `%m/%d/%Y`, `%Y/%m/%d`, `%d-%b-%Y`… with and without time; keep the formats that parse everything | `DATE` / `TIMESTAMP` via `try_strptime(x, fmt)` |
| Ambiguous date order | both day-first and month-first parse every value (no day above 12) | use the "Date order" setting (default from the UI language), and mark the badge with `?` so the user can flip it in one click |
| JSON | every value starts with `{` or `[` and `json_valid` | `JSON` |
| Empty column | no non-empty value in the sample | text |

Epoch numbers are never turned into dates automatically (too many false positives). That is a conversion the user picks (§4).

### 3.3 What the user sees and controls

- Type badges in the headers. Columns where the sample needed a decision (ambiguous dates, locale numbers) show a dot. Columns with invalid values show a count.
- **The detected types are part of the source, not a step.** That keeps cell edits allowed (edits need a tab without transforms). Changing a column's source type from its badge just recreates the `__pv_src_*` view (cheap), and it can be undone.
- ParVu remembers per file the types the user changed (in `~/.ParVu/column_types.json`, keyed by path + size + modified time) and applies them the next time that file opens.
- New settings: "Detect column types when opening" (on), "Date order: automatic / day first / month first", "Decimal separator: automatic / . / ,".
- P1: an **Open with options…** dialog (File menu): delimiter, header row, encoding, decimal separator, date order, skip rows, and the type of each column, with a preview of the first 50 rows.

## 4. Conversions

### 4.1 Operations

```rust
/// Change a column's type. Never fails: values that don't convert become
/// NULL (or `on_error`), and can be listed with ConversionErrors.
Convert {
    column: String,
    target: String,              // checked with is_safe_type_name
    #[serde(default)] parse: ParseOpts,   // Auto | Formats(Vec<String>) | Epoch(Unit) | Locale { decimal, thousands, strip, percent } | Words { true_words, false_words }
    #[serde(default)] tz: Option<String>, // TIMESTAMPTZ only
    #[serde(default)] on_error: OnError,  // Null | Value(Cell)
    #[serde(default)] keep_original: bool, // adds "<column>_raw" right after it
}
/// Keep (or remove) the rows whose value doesn't convert: Power Query's
/// Keep Errors / Remove Errors.
ConversionErrors { column: String, target: String, parse: ParseOpts, keep: bool }
```

`ChangeType` stays for compatibility but is built as `TRY_CAST`. Export's per-column cast becomes `TRY_CAST` too. Every format string and word goes through `quote_literal`. Both variants pass `readonly::check_operation` (only literals and safe type names), so AI clients can use them; add injection tests to `tests/readonly.rs`.

### 4.2 Conversion matrix (what `Convert` generates)

| From → to | SQL | Can change values |
|---|---|---|
| text → integer | `TRY_CAST(trim(x) AS BIGINT)`; strict mode adds `regexp_full_match(trim(x), '[+-]?\d+')` | `'12.7'` rounds to 13 unless strict |
| text → number | `TRY_CAST(replace(replace(trim(x), thou, ''), dec, '.') AS DOUBLE)`, symbols stripped first | — |
| text → decimal(p,s) | the same `AS DECIMAL(p,s)` | rounds to scale |
| text → boolean | `CASE WHEN lower(trim(x)) IN (true words) THEN true WHEN … IN (false words) THEN false END` | — |
| text → date / timestamp | `try_strptime(trim(x), ['fmt1', 'fmt2'])`, `::DATE` for dates; ISO via `TRY_CAST` | text → date drops a time part (warned) |
| text → JSON | `TRY_CAST(x AS JSON)` | — |
| text → list | `TRY_CAST(TRY_CAST(x AS JSON) AS T[])` | — |
| integer → timestamp | `epoch_ms(n)`, `to_timestamp(n)` (seconds, gives TIMESTAMPTZ), `make_timestamp(n)` (µs) | — |
| number → integer | `TRY_CAST` | rounds half to even (warned) |
| timestamp → date | `CAST(ts AS DATE)` | drops the time (warned) |
| timestamptz → timestamp | `timezone(tz, ts)`; never an implicit session zone | — |
| date / timestamp → text | `strftime(ts, fmt)` | — |
| number → text | `CAST(x AS VARCHAR)` or `format('{:,.2f}', x)` | formatted text can't go back |
| list / struct → text | `to_json(x)::VARCHAR` (round-trips; plain `CAST` doesn't) | — |
| date → number | not offered; use "epoch" from the Date menu | — |

### 4.3 Errors

- When a conversion step is applied, count errors on the first 100,000 rows: `count(*) FILTER (WHERE x IS NOT NULL AND trim(x) <> '' AND conv IS NULL)`. This is instant. The status bar shows "≈ 12 values in `price` didn't convert (first 100,000 rows) · Count all · Show".
- **Count all** runs the same query over everything in the background (one column read; seconds on a multi-GB Parquet file) and can be cancelled.
- **Show** applies `ConversionErrors { keep: true }`, an ordinary undoable step that streams lazily.
- The grid shows failed cells like NULL. P2: an `invalid` mask on `Page` (like `edited`), computed for the visible page only, to draw them in the error colour.

### 4.4 The Convert dialog

- Target type (number, integer, decimal with precision, text, boolean, date, timestamp, time, duration, JSON).
- How to read it:
  - automatic;
  - date formats (chips, with the detected format preselected);
  - epoch unit;
  - decimal and thousands separators;
  - true/false words;
  - time zone (only for TIMESTAMPTZ).
- On error: empty or a value. Keep the original column.
- **Live preview:** 50 rows of `SELECT x, conv FROM (current query) LIMIT 50`, original → converted, failed cells marked, "3 of 50 failed", and a warning when values change (rounding, a dropped time).

## 5. Operations by type

### 5.1 Shared machinery

One new variant for every per-column function, so `ops.rs` stays small and AI clients get one schema:

```rust
ColumnTransform {
    column: String,
    func: ColumnFn,          // enum: Trim{side}, Lower, Upper, Proper, Prefix{text}, Extract{part}, StartOf{unit}, JsonPath{path}, …
    #[serde(default)] output: Output,  // InPlace | New { name }  (new columns go right after the source)
}
```

- `InPlace` builds `SELECT * REPLACE (expr AS "c")`. The existing `AddColumn` already does this when the name exists (`ops.rs:468`).
- `New` builds an explicit column list, so the new column sits next to its source as in Power Query. The name defaults to `c_year`, `c_length`… via `unique_name`.
- Each `ColumnFn` has a `StepLabel` and a test in `ops.rs`.
- **Preview command** `preview_column_fn(tab, column, func) → [(before, after); 5]`. It evaluates the expression over the five values already on the current page: `SELECT v, expr FROM (VALUES (...)) t(v)`, with `v` cast to the column type. This takes microseconds whatever the file size. (`preview_op` counts every row, so it is too slow here.)

### 5.2 Text

| Operation | Default | SQL (x = the column) | Priority |
|---|---|---|---|
| Trim (both, start, end; also tabs and line breaks) | in place | `trim(x)`, `ltrim`, `rtrim` with `' '\|\|chr(9)\|\|chr(10)\|\|chr(13)` | P0 |
| Clean (non-printable characters) | in place | `regexp_replace(x, '[[:cntrl:]]', '', 'g')` | P0 |
| lowercase, UPPERCASE, Capitalize Each Word | in place | `lower`, `upper`; proper case with `list_transform(string_split(lower(x), ' '), w -> upper(w[1]) \|\| w[2:])` | P0 |
| Add prefix / suffix | in place | `'p' \|\| x`, `x \|\| 's'` | P0 |
| Length | new | `length(x)` | P0 |
| First / last N characters | new | `left(x, n)`, `right(x, n)` | P0 |
| Text before / after a delimiter (first or last occurrence) | new | `split_part(x, d, 1)`, `split_part(x, d, -1)`; strict variant returns empty when the delimiter is missing | P0 |
| Merge columns | new | `concat_ws(sep, a, b, …)` (skips NULLs) | P0 |
| Text → boolean with words | in place | see §4.2 | P0 |
| Collapse repeated spaces | in place | `regexp_replace(trim(x), '\s+', ' ', 'g')` | P1 |
| Pad start / end | in place | `lpad(x, n, '0')`, `rpad` | P1 |
| Range of characters | new | `substring(x, start, len)` | P1 |
| Text between two delimiters | new | `split_part(split_part(x, a, 2), b, 1)` | P1 |
| Regex extract (group n) | new | `regexp_extract(x, pattern, n)` | P1 |
| Remove accents | in place | `strip_accents(x)` | P1 |
| Contains / starts with / matches, as a boolean column | new | `contains(x, v)`, `starts_with`, `regexp_matches` | P1 |
| Split by number of characters or by positions | columns | `substring` per part (completes `SplitColumn`) | P1 |
| Split into rows | rows | `SELECT * REPLACE (unnest(string_split(x, d)) AS "c")`; warns that it multiplies rows | P1 |
| Position of text | new | `strpos(x, s)` | P2 |
| Similarity to a value | new | `jaro_winkler_similarity(x, v)`, `levenshtein(x, v)` | P2 |

Text filters already cover contains, starts/ends with, regex, in list and empty (`clean.rs:19`); nothing to add there.

### 5.3 Boolean

| Operation | SQL | Priority |
|---|---|---|
| Invert | `NOT x` | P0 |
| Keep true / false / empty (one click) | `Filter` with `=` / `is_null` | P0 |
| Show as labels (Yes/No…) | `CASE WHEN x THEN 'Yes' WHEN NOT x THEN 'No' END` | P1 |
| Number → boolean | `x <> 0` | P1 |
| Combine columns (and, or, either) | `a AND b`, `a OR b`, `a <> b` | P1 |
| "Is empty" flag for any column | `x IS NULL` | P2 |

Counts (value counts), fill empty and statistics already work for booleans; statistics gains "% true".

### 5.4 Date and time

| Operation | Default | SQL | Priority |
|---|---|---|---|
| Year, quarter, month, day, day of year | new | `year(x)`, `quarter`, `month`, `day`, `dayofyear` | P0 |
| Month name, day name | new | `monthname`, `dayname` are English only, so translate with a `CASE` from the locale files | P0 |
| Day of week (Monday = 1), ISO week, year-week | new | `isodow`, `week`, `yearweek` | P0 |
| Hour, minute, second | new | `hour`, `minute`, `second` | P0 |
| Date only / time only (from a timestamp) | in place | `CAST(x AS DATE)`, `CAST(x AS TIME)` | P0 |
| Start of year, quarter, month, week, day | in place | `date_trunc(unit, x)`, cast back to `DATE` for date columns | P0 |
| Age, or days since today | new | `date_diff('day', x, current_date)` | P0 |
| Difference between two date columns (days, months, years) | new | `date_diff(unit, a, b)` (calendar boundaries: 31 Jan → 1 Feb is 1 month) | P0 |
| Relative filters: last N days/weeks/months, this week/month/quarter/year, year to date | filter | written as a range (`x >= start AND x < end`) so Parquet row groups are skipped | P0 |
| End of month, quarter, year | in place | `last_day(x)`; `date_trunc('quarter', x) + INTERVAL 3 MONTH - INTERVAL 1 DAY` | P1 |
| Add or subtract an interval | in place | `x + INTERVAL (n) DAY` (MONTH, HOUR…) | P1 |
| Combine date and time columns | new | `d + t` | P1 |
| Epoch → timestamp | in place | `epoch_ms(n)`, `to_timestamp(n)`, `make_timestamp(n)` | P1 |
| Format as text | new | `strftime(x, fmt)` with presets | P1 |
| Is weekend / is weekday | new | `isodow(x) >= 6` | P1 |
| Timestamp → epoch | new | `epoch(x)`, `epoch_ms(x)` | P2 |
| Convert time zone | in place | `x AT TIME ZONE 'tz'` (needs ICU in installers, §0) | P2 |
| Duration in days, hours, seconds | new | `epoch(x) / 86400`… | P2 |

### 5.5 JSON and nested columns

| Operation | Default | SQL | Priority | Cost |
|---|---|---|---|---|
| Struct fields → columns | in place | `SELECT * EXCLUDE (c), c.* …`; fields come from the type, no sampling | P0 | free |
| Expand a JSON object into columns | in place | `json_transform(x, '{"k":"VARCHAR",…}')` then `.*` (parses each value once) | P0 | one parse per row |
| Extract a path | new | `json_extract_string(x, '$.a.b')` (`->>`) or `json_extract` (`->`) | P0 | lazy |
| Text → JSON | in place | `TRY_CAST(x AS JSON)`; "keep invalid rows" via `json_valid` | P1 | lazy |
| List or JSON array → rows | rows | `SELECT * REPLACE (unnest(x) AS "c")`; JSON via `CAST(x AS JSON[])`; warns about row growth | P1 | multiplies rows |
| List length | new | `len(x)`, `json_array_length(x)` | P1 | free |
| List → text | in place | `array_to_string(x, ', ')` | P1 | free |
| Keys of an object | new | `json_keys(x)` | P2 | |

**Finding the keys:**
- `SELECT json_group_structure(x) FROM (current query) LIMIT 2000` merges the keys and types of the first 2,000 values. It is instant.
- The dialog shows the result as a tree with checkboxes, which doubles as the path picker. "Scan all rows" runs it without the limit in the background.
- The chosen keys are stored in the step. Keys missing from a row give NULL; keys missing from the sample can be typed in by hand.

### 5.6 Numbers (small set, for completeness)

Round to N places, round down, round up, absolute value, sign, percent of the column total (window sum), and number → text with thousands separators. All of these are P1 and per row, except percent of total, which is one window pass.

## 6. UI

- **Type-aware column menu** (`headerMenu`, `App.tsx:595`). Above the common items, insert one block for the column's family:
  - Text: **Text ▸** (Trim, Clean, case, Prefix…, Suffix…, Pad…), **Extract ▸** (Length, First N…, Last N…, Before/After delimiter…, Between…, Regex…), **Split ▸** (existing, into rows…).
  - Date/Timestamp: **Date ▸** (Year, Quarter, Month, Month name, Week, Day, Day name, Day of week, Date only), **Start/End of ▸**, Age / Difference…, Add interval…, Format…. Plus a **Time ▸** block for timestamps and times.
  - Boolean: Invert, Keep true / false / empty, Show as labels….
  - JSON/List/Struct: Expand…, Extract path…, To rows, Length, Join to text.
  - Every column: **Change type ▸**, the badge's menu: the common targets, then Convert… with options, and Detect type.
- One-click items need no dialog (undo is the safety net). Items with "…" open one shared `ColumnFnDialog`: one or two inputs, "Same column / New column [name]", and the five-row before → after preview.
- The filter dialog gets date presets for date columns and true/false/empty chips for booleans.
- The same functions get DSL names for the expression editor: LTRIM, RTRIM, PROPER, STRIP_ACCENTS, CONCAT_WS, POSITION, TRY_STRPTIME, MONTHNAME, DAYNAME, LAST_DAY, EPOCH, JSON_EXTRACT, JSON_EXTRACT_STRING. Update `tests/golden/dsl.json` where compiled output changes.
- All new labels go into the en, ru and az locale files.

## 7. Phases

Each phase is one PR with tests, docs and changelog.

| Phase | Content | Size |
|---|---|---|
| **0. Safety** | `ChangeType` and export use `TRY_CAST`; split column without a count reads a 10k-row sample; add `duckdb/icu` to `bundled` (measure the installer size) | S |
| **1a. Detection (engine)** | `TypeFamily` in `ColumnMeta`; detectors (§3.2); native columns plus text columns where needed; healing; `type_pushdown` off; tests with deep bad values, ambiguous dates, locale numbers, yes/no, leading zeros | M |
| **1b. Types in the UI** | header badges; change a column's source type; date-order and decimal settings; per-file remembered types | M |
| **2. Conversions** | `Convert` and `ConversionErrors`; the Convert dialog with preview; sample error count, background exact count, "Show invalid rows" | M |
| **3. Text and boolean** | `ColumnTransform` + preview command; type-aware menu; text and boolean P0 | M |
| **4. Dates** | date and time P0; relative date filters; translated month and day names | M |
| **5. JSON and nested** | struct expand, JSON expand with key sampling, extract path, P1 nested ops | M |
| **6. Rest** | P1/P2 items, Open with options… dialog, invalid-cell colouring, DSL functions | M |

**Performance gates for every phase:**
- Opening a file stays within +50 ms of today.
- No new full scan on open or when a menu opens.
- A benchmark test (ignored by default, run on demand) on a generated 2M-row CSV, comparing first page, sum and sort with and without the change.
- Generated test files stay small (≤ 200 MB). `/tmp` on developer machines may be in RAM, so experiments cap DuckDB with `SET memory_limit`.

## 8. Decisions to make

| Question | Recommendation |
|---|---|
| Should the detected types be a visible step (Power Query's "Changed Type") or part of the source? | Part of the source, shown by header badges. A step would lock cell editing. |
| Sample size for detection | 10,000 rows from the start of the file (≈ 40 ms). Power Query uses 200, which misses too much. |
| Default for ambiguous dates | From the UI language (en → month first, ru/az → day first), marked with `?` on the badge. |
| Should change type silently become safe (`TRY_CAST`)? | Yes, with the error count in the status bar. A strict mode would bring back the crashes. |
| Should Extract add the column after its source or at the end? | After its source, as Power Query does. |
| One `ColumnTransform` variant or one variant per operation? | One, with a `ColumnFn` enum. |
| Should relative date filters store "today" or the date when applied? | Store "today" (the filter means "last 7 days"), and show the resolved range in the step label. |
| Expanded JSON column names: `payload.a` or `a`? | `a`, falling back to `payload.a` on a clash. |
| Add ICU to installers (time zones)? | Yes, if the installer grows by under ~5 MB; otherwise time zones stay development-only. |
| Holiday calendars for "business day"? | Out of scope; offer weekend / weekday only. |
