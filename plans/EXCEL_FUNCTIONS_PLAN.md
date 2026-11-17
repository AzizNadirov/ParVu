# Excel functions for the expression language

Goal: people who know Excel can write ParVu formulas and expressions with the function names they already know. The 100 most-used Excel functions either work under their Excel name, or the reference says what to use instead.

## 1. Decisions

| Question | Decision |
|---|---|
| Names | Excel's own names and argument order (`AVERAGE`, `COUNTIF`, `TEXTJOIN`, `EOMONTH`…), case-insensitive. Native names (`AVG`…) keep working. |
| Collisions | `REPLACE` stays ParVu's (text, pattern, replacement); Excel's positional `REPLACE(text, start, n, new)` is not added (`LEFT`/`MID`/`RIGHT` cover it). Every other Excel name is free or already means the same. |
| Ranges | A column `[x]` plays the role of `A:A`. Inside an aggregate it is the column total (as `SUM([x])` today); a range `[a:c]` is a row (as `SUM([q1:q4])` today). |
| Criteria | `COUNTIF([region], "North")`, `SUMIF([qty], ">10")`, `"<>"`, `"*text*"` wildcards: Excel's criterion strings, parsed when the formula compiles. |
| Lookups | `XLOOKUP(value, other[key], other[result], [if_not_found])` and `VLOOKUP(value, other[key], other[result])` look up in another open tab (a correlated subquery; DuckDB turns it into a join: 300,000 lookups into 4,000 keys take 10 ms). |
| Row-relative totals | `COUNTIF([region], [region])` (how many rows share this row's region) and the other conditional totals with a criterion from the row become window functions over the whole table; like `PREV`, allowed up to 2,000,000 rows. |
| `TEXT` | `TEXT(value, format)` understands the common Excel format codes: `0`, `0.00`, `#,##0.00`, `0%`, `0.0%`, `dd.mm.yyyy`, `yyyy-mm-dd`, `mmm yyyy`, `dddd`, `hh:mm`… |
| Where to find them | A new **Excel functions** tab in Help → Expression Language Guide: every function by Excel category, its ParVu form, an example, and for the ones that don't apply, what to use instead (a menu, an operation). The same table goes to `docs/DSL.md` and the wiki. |

## 2. How it is built

- `FunctionDef` gains an optional **SQL template** (`{0}`, `{1}`… for arguments), compiled through `Expr::Template` (added for ranges in F4a). Most Excel functions are one template: `PROPER`, `MID`, `FIND`, `EOMONTH`, `ROUNDUP`…
- **Variadic** functions (`IFS`, `SWITCH`, `CHOOSE`, `AND`, `OR`, `XOR`, `TEXTJOIN`, `CONCATENATE`) are built in the resolver, like `PREV`.
- **Aggregates** (`AVERAGE`, `MEDIAN`, `COUNTIF`…) join the aggregate list, so in formulas they become cached column totals (F4a).
- **Criteria** are parsed from a string literal into a SQL condition at compile time (`">=10"` → `x >= 10`, `"a*"` → `x LIKE 'a%'`).
- The Excel catalogue (`excel.rs` in parvu-dsl) lists all 100 names with category, ParVu form, example and note; the UI shows it, and a test checks that every name marked supported compiles.

## 3. The 100 functions

**S** = works under its Excel name, **N** = native ParVu name already does it (alias added), **M** = use a menu or operation instead.

| Category | Functions |
|---|---|
| Math (22) | SUM N, SUMPRODUCT S, PRODUCT S, ROUND N, ROUNDUP S, ROUNDDOWN S, MROUND S, INT S, TRUNC S, CEILING S, FLOOR S, ABS N, SIGN N, MOD N, POWER N, SQRT N, EXP N, LN N, LOG S, LOG10 N, PI S, QUOTIENT S |
| Random (2) | RAND S, RANDBETWEEN S |
| Statistics (18) | AVERAGE S, COUNT N, COUNTA S, COUNTBLANK S, MIN N, MAX N, MEDIAN S, MODE S, STDEV S, STDEV.P S, VAR S, VAR.P S, LARGE S, SMALL S, PERCENTILE S, QUARTILE S, RANK M (Operations → Ranks), CORREL S |
| Conditional totals (8) | COUNTIF S, COUNTIFS S, SUMIF S, SUMIFS S, AVERAGEIF S, AVERAGEIFS S, MAXIFS S, MINIFS S |
| Logical (12) | IF N, IFS S, IFERROR S, IFNA S, AND S, OR S, NOT S, XOR S, SWITCH S, CHOOSE S, TRUE N, FALSE N |
| Information (6) | ISBLANK S, ISNUMBER S, ISTEXT S, ISEVEN S, ISODD S, NA S |
| Text (20) | LEFT N, RIGHT N, MID S, LEN N, FIND S, SEARCH S, SUBSTITUTE S, UPPER N, LOWER N, PROPER S, TRIM N, CLEAN S, CONCAT N, CONCATENATE S, TEXTJOIN S, TEXT S, VALUE S, REPT S, EXACT S, TEXTBEFORE S, TEXTAFTER S |
| Date and time (20) | TODAY N, NOW N, DATE S, TIME S, YEAR N, MONTH N, DAY N, HOUR N, MINUTE N, SECOND S, WEEKDAY S, WEEKNUM S, ISOWEEKNUM S, EDATE S, EOMONTH S, DATEDIF S, DAYS S, NETWORKDAYS S, YEARFRAC S, DATEVALUE S |
| Lookup (6) | XLOOKUP S, VLOOKUP S, INDEX M, MATCH M, HLOOKUP M, LOOKUP M |
| Arrays and others (6) | FILTER M (Filter Rows), SORT M (Sort), UNIQUE M (Unique Values / Drop Duplicates), TRANSPOSE M (Transpose), SUBTOTAL M (Group By), OFFSET M |

About 85 work as functions; the rest point to the menu that does the job.

## 4. Phases

| Phase | Content |
|---|---|
| **X1** | Templates in the registry; math, random, information, text, date functions; logical and variadic functions; aliases. Golden tests for each. |
| **X2** | Statistics and conditional totals (criteria parser); formulas treat them as totals. |
| **X3** | XLOOKUP / VLOOKUP across tabs. |
| **X4** | The Excel catalogue, the **Excel functions** help tab, more examples in the guide, `docs/DSL.md` and the wiki. |

One PR, phases as commits.

## 5. Status

Done: X1–X4 in one PR. 157 functions in the registry (85 new), 124 Excel names in the catalogue. Tested by running every function in DuckDB (`parvu-app/src/excel_tests.rs`), compiling every example (registry and guide), and unit tests for criteria and format codes. Also: a single `=` compares; dotted names (`STDEV.P`); table ranges (`prices[id:price]`).
