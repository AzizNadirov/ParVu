# ParVu Expression Language (DSL)

ParVu includes a small domain-specific language (DSL) for data transformations. It compiles to DuckDB SQL behind the scenes, giving you the power of SQL with a more concise, Python-like syntax.

## Table of Contents

- [Overview](#overview)
- [Dual-Mode Editor](#dual-mode-editor)
- [Syntax](#syntax)
  - [Column References](#column-references)
  - [Assignment](#assignment)
  - [Literals](#literals)
  - [Operators](#operators)
  - [Function Calls](#function-calls)
- [Built-in Functions](#built-in-functions)
  - [Math](#math)
  - [String](#string)
  - [Aggregation](#aggregation)
  - [Special](#special)
- [Examples](#examples)
- [Auto-Completion](#auto-completion)
- [Error Handling](#error-handling)

## Overview

The expression language lets you write data transformations without writing full SQL:

```python
data[total] = price * quantity
```

This compiles to:

```sql
SELECT *, price * quantity AS "total" FROM data
```

## Dual-Mode Editor

The query editor has two modes:

- **SQL Mode** (default) — Write raw DuckDB SQL
- **Expression Mode** — Write DSL expressions

Switch modes using the toggle in the query toolbar.

## Syntax

### Column References

Reference columns using bracket notation:

```python
data[column_name]      # table + column
data["column name"]    # quoted for names with spaces
```

The table name is optional if there's only one table loaded — the editor infers it.

### Assignment

Add a new column (or overwrite an existing one):

```python
data[discount] = price * 0.15
data[full_name] = first_name || ' ' || last_name
```

Compilation:
```sql
SELECT *, price * 0.15 AS "discount" FROM data
```

### Literals

- **Numbers**: `42`, `3.14`
- **Strings**: `'hello'`, `"world"`
- **Booleans**: `true`, `false` ( DuckDB style )
- **NULL**: `null`

### Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `+` | Addition | `price + tax` |
| `-` | Subtraction | `price - discount` |
| `*` | Multiplication | `qty * price` |
| `/` | Division | `total / count` |
| `||` | Concatenation | `first || ' ' || last` |
| `=` | Equality | `status = 'active'` |
| `!=` | Not equal | `status != 'deleted'` |
| `<`, `>` | Less / Greater | `age > 18` |
| `<=`, `>=` | Less/Greater or equal | `score >= 90` |

### Function Calls

```python
UPPER(data[name])
ROUND(data[price], 2)
LEN(data[description])
```

Functions are case-insensitive: `upper`, `UPPER`, `Upper` all work.

## Built-in Functions

### Math

| Function | Description | Example |
|----------|-------------|---------|
| `ABS(x)` | Absolute value | `ABS(data[value])` |
| `ROUND(x, n)` | Round to n decimals | `ROUND(data[price], 2)` |
| `CEIL(x)` | Ceiling | `CEIL(data[score])` |
| `FLOOR(x)` | Floor | `FLOOR(data[score])` |
| `SQRT(x)` | Square root | `SQRT(data[area])` |
| `POWER(x, y)` | x raised to y | `POWER(data[base], 2)` |
| `LN(x)` | Natural log | `LN(data[value])` |
| `EXP(x)` | e^x | `EXP(data[rate])` |

### String

| Function | Description | Example |
|----------|-------------|---------|
| `UPPER(s)` | Uppercase | `UPPER(data[name])` |
| `LOWER(s)` | Lowercase | `LOWER(data[email])` |
| `LEN(s)` | Length | `LEN(data[code])` |
| `TRIM(s)` | Remove whitespace | `TRIM(data[comment])` |
| `SUBSTRING(s, start, len)` | Extract substring | `SUBSTRING(data[id], 1, 4)` |
| `REPLACE(s, old, new, case_sensitive, regex)` | Replace text | `REPLACE(data[phone], '-', '')` |
| `CONTAINS(s, sub)` | Contains check | `CONTAINS(data[desc], 'sale')` |
| `STARTSWITH(s, prefix)` | Prefix check | `STARTSWITH(data[id], 'US')` |
| `ENDSWITH(s, suffix)` | Suffix check | `ENDSWITH(data[file], '.csv')` |

### Aggregation

| Function | Description | Example |
|----------|-------------|---------|
| `SUM(x)` | Sum | `SUM(data[amount])` |
| `AVG(x)` | Average | `AVG(data[score])` |
| `COUNT(x)` | Count | `COUNT(data[id])` |
| `MIN(x)` | Minimum | `MIN(data[date])` |
| `MAX(x)` | Maximum | `MAX(data[date])` |
| `STDDEV(x)` | Standard deviation | `STDDEV(data[error])` |
| `VARIANCE(x)` | Variance | `VARIANCE(data[error])` |

### Special

| Function | Description | Example |
|----------|-------------|---------|
| `DROP_DUPLICATES(table, cols..., keep)` | Remove duplicates | `DROP_DUPLICATES(data[id], data[name], 'first')` |
| `DROP_NULL(col, null_value=NULL)` | Drop rows where col is NULL (or equals a sentinel) | `DROP_NULL(data[score], -1)` |

#### REPLACE

Replace occurrences of a pattern in text. Supports literal and regex replacement, with optional case-insensitive matching.

```python
# Simple literal replacement (case-sensitive, default)
REPLACE(data[brand], 'no brand', '-')

# Case-insensitive literal replacement
REPLACE(data[brand], 'No Brand', '-', FALSE)

# Regex replacement
REPLACE(data[phone], '[0-9]+', '#', TRUE, TRUE)
```

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `text` | TEXT | ✅ | Source string |
| `pattern` | TEXT | ✅ | Search pattern |
| `with_value` | TEXT | ✅ | Replacement string |
| `case_sensitive` | BOOLEAN | ❌ | Default `TRUE` |
| `regex` | BOOLEAN | ❌ | Default `FALSE` |

**SQL compilation:**
- `case_sensitive=TRUE, regex=FALSE` → `REPLACE(text, pattern, with_value)`
- `case_sensitive=FALSE, regex=FALSE` → `REGEXP_REPLACE(text, pattern, with_value, 'gi')` (pattern escaped for literal matching)
- `regex=TRUE` → `REGEXP_REPLACE(text, pattern, with_value, 'g')` or `'gi'`

Also available as a method:
```python
data[brand].replace('old', 'new', FALSE, FALSE)
```

#### DROP_DUPLICATES

Remove duplicate rows based on a subset of columns:

```python
# Keep first occurrence (default)
DROP_DUPLICATES(data[id])

# Keep last occurrence
DROP_DUPLICATES(data[id], data[category], 'last')

# Multiple columns
DROP_DUPLICATES(data[first_name], data[last_name], data[birth_date], 'first')
```

Compilation (keep='first'):
```sql
SELECT * FROM data
QUALIFY ROW_NUMBER() OVER (PARTITION BY id) = 1
```

Compilation (keep='last'):
```sql
SELECT * FROM data
QUALIFY ROW_NUMBER() OVER (PARTITION BY id ORDER BY rowid DESC) = 1
```

If only one column is provided, all columns from the table are used as the partition key.

#### DROP_NULL

Drop rows where a column equals a "null sentinel". Two modes:

```python
# Mode 1 (default): null_value omitted -> drop SQL NULLs
DROP_NULL(data[email])

# Mode 2: explicit sentinel -> drop rows where col equals that value;
# real SQL NULLs are kept (chain another DROP_NULL with no sentinel
# to remove them too).
DROP_NULL(data[score], -1)
DROP_NULL(data[name], '')
DROP_NULL(data[status], 'N/A')
```

Compilation:

```sql
-- DROP_NULL(data[email])
SELECT * FROM data WHERE NOT email IS NULL

-- DROP_NULL(data[score], -1)
SELECT * FROM data WHERE score IS NULL OR score <> -1
```

The first argument must be a column reference. The second (optional) must be a
literal (string, integer, or float). Available in the UI under
**Operations → Drop Null Values...** and via the column header context menu.

## Examples

### Add Computed Columns

```python
data[revenue] = data[price] * data[qty]
data[discounted] = data[price] * (1 - data[discount_rate])
data[category_upper] = UPPER(data[category])
```

### Filter with Expressions

In SQL mode you can reference expression results:

```sql
SELECT * FROM data WHERE revenue > 1000
```

### String Manipulation

```python
data[domain] = REPLACE(SUBSTRING(data[email], LEN(data[email]) - 10, 10), '@', '')
data[clean_phone] = REPLACE(REPLACE(data[phone], '-', ''), ' ', '')
```

### Deduplication

```python
# Keep first row per customer
DROP_DUPLICATES(data[customer_id], 'first')

# Keep most recent per category
DROP_DUPLICATES(data[category], data[product_id], 'last')
```

## Auto-Completion

The expression editor provides context-aware suggestions:

- **Functions** — Type `DR` and see `DROP_DUPLICATES`
- **Columns** — Type `data[` and see available columns
- **Tables** — Type a table name prefix for multi-table scenarios
- **Function docs** — Navigate the completer with arrow keys to see signature, parameters, description, example, and SQL mapping in a side popup. Click a function name in the editor to show its documentation.
- **Methods** — Type `data[name].` to see available methods (e.g., `.upper()`, `.replace()`)

Suggestions appear after typing 2+ characters. Press **Tab** or **Enter** to accept.

## Error Handling

Common errors and how to fix them:

| Error | Cause | Fix |
|-------|-------|-----|
| `Unknown function` | Typo or unsupported function | Check spelling or use SQL mode |
| `Column not found` | Wrong column name | Check column list or use quotes for spaced names |
| `Type mismatch` | e.g. `UPPER(123)` | Ensure correct argument types |
| `Invalid keep value` | `DROP_DUPLICATES` with wrong keep | Use `'first'` or `'last'` |

Errors are shown inline in the editor status bar.

---

**Version**: 0.2.0+
**Last Updated**: 2026-05-26
