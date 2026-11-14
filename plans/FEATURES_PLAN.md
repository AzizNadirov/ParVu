# Pandas and spreadsheet features

Goal: cover the everyday pandas and LibreOffice Calc operations people use on a single table, without leaving ParVu. Every table operation is an `Operation` (SQL over the tab's current query, a step label, undo for free); view features (charts, formatting, frozen columns) live in the UI and do not change the data.

## 1. Reshaping

| Feature | pandas / Calc | Operation |
|---|---|---|
| Group by and aggregate | `groupby().agg()`, Calc subtotals | `group_by` (count, rows, distinct, sum, mean, median, min, max, std, var, first, last, mode, list) |
| Pivot table | `pivot_table`, `crosstab`, Calc pivot | `pivot` (one value column per distinct value, up to 200) |
| Unpivot | `melt` | `melt` |
| Transpose | `df.T` | `transpose` (up to 500 rows) |
| Value counts | `value_counts` | `value_counts` (optional proportion) |
| Summary of every column | `describe` | `describe` (DuckDB `SUMMARIZE`) |

## 2. Filter and clean

| Feature | pandas / Calc | Operation |
|---|---|---|
| Filter rows by conditions | boolean indexing, `query`, Calc standard filter | `filter` (=, ≠, <, ≤, >, ≥, between, contains, starts/ends with, regex, in list, is null/empty; AND or OR) |
| Fill missing values | `fillna`, `ffill`, `bfill` | `fill_null` (value, forward, backward, mean, median, mode) |
| Sort by several columns | `sort_values([...])` | `sort_by` |
| Choose and reorder columns | `df[[...]]`, `drop(columns=)` | `select_columns` |
| First / last / random rows | `head`, `tail`, `sample` | `slice` |
| Split a column | `str.split(expand=True)`, Calc Text to Columns | `split_column` |
| Binning | `cut`, `qcut` | `bin` (equal width, quantiles, custom edges) |

## 3. Column functions

| Feature | pandas | Where |
|---|---|---|
| Cumulative sum/min/max/count | `cumsum`, `cummax`, `cummin`, `cumcount` | `window` operation |
| Shift, difference, percent change | `shift`, `diff`, `pct_change` | `window` |
| Rank | `rank(method=…)` | `window` (average, min, max, dense, first) |
| Rolling mean/sum/min/max/std | `rolling(n).mean()` | `window` |
| Group-wise versions of the above | `groupby().cumsum()` | `window` with partition columns |
| Conditionals | `np.where`, Calc `IF` | `IF`, `IFNULL`, `GREATEST`, `LEAST` in expressions |
| More text functions | `str.split/slice/pad/extract` | `SPLIT_PART`, `SUBSTR`, `LEFT`, `RIGHT`, `LPAD`, `RPAD`, `REVERSE`, `REGEX_EXTRACT`, `REGEX_MATCH` |
| More date functions | `dt.strftime/quarter/week/floor` | `STRFTIME`, `STRPTIME`, `QUARTER`, `WEEK`, `DAYOFWEEK`, `HOUR`, `MINUTE`, `DATE_TRUNC` |
| More math | `np.log` etc. | `LN`, `LOG10`, `EXP`, `MOD`, `SIGN` |

## 4. Spreadsheet features (view only, not steps)

- Charts: bar, line, scatter, histogram, with the aggregation done in DuckDB so big files work.
- Conditional formatting: colour scales and highlight rules per column.
- Freeze the first columns.
- Column widths remembered per tab.
- Fill down over a selection (one batch of cell edits).

## Status

- [ ] 1. Reshaping
- [ ] 2. Filter and clean
- [ ] 3. Column functions
- [ ] 4. Spreadsheet features
