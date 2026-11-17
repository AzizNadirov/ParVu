# ParVu Quick Start

## Open a file

- Drop one or more files onto the window, or
- File → Open (`Ctrl+O`), or
- paste a path on the start screen and press Enter, or
- run `parvu file.parquet other.csv`.

Each file opens in its own tab at the bottom of the window. The table is named after the file: `sales 2024.csv` becomes `sales_2024`.

An Excel workbook opens on its first visible sheet. The **Sheet: … ▾** button next to the table name lists all sheets: show another one in the tab, open sheets in their own tabs, or rename, hide, duplicate, reorder and delete sheets (saved with the workbook).

## Look around

- Move between pages with the pagination bar, or type a page number.
- Click a column header to sort; click again to reverse.
- Right-click a header for the column's menu: sort and filter at the top, then groups (**Column**, **Type**, **Transform**, **Clean**, **Analyze**, **New Table from This Column**, **Display**) and the type's own functions (**Text**, **Dates**, **JSON**…). The **Column** menu in the menu bar holds the same groups for the selected column.
- `Ctrl+F` searches every row on every page.

The **SQL Query** panel starts collapsed to one line: the table name and the quick buttons (Table Info, Filter, Group by, Chart, Formula, Join, Append). Click the arrow by its title to open the editor. **View** shows or hides the panel and **Applied Steps**; ParVu remembers your choice (also in **Settings → General → View**).

## Query

Write SQL in the editor and press **Execute** or `Ctrl+Enter`:

```sql
SELECT region, COUNT(*) AS orders
FROM sales
GROUP BY region
ORDER BY orders DESC
```

Or switch the editor to **Expression** to add columns without writing SQL:

```text
sales[total] = sales[qty] * sales[price]
```

Every query and operation is listed under **Applied Steps**. `Ctrl+Z` undoes the last one, and **Reset Query** returns to the original file.

Tick **Result in new tab** next to Execute Query to open the query's result in a new tab instead of changing this one.

## Column types

Each column header shows an icon for its type: numbers, text, yes/no, dates, times, JSON. Text columns that hold dates, numbers or yes/no words get those types when a file opens (CSV, Parquet, JSON or Excel). Right-click a header and choose **Read as** to read the column another way, for example a number written `1.234,56`, or a date written `31.12.2024`. A calendar icon with a question mark means the day and month order was a guess; **Read dates day first / month first** in the same menu flips it. ParVu remembers your choice for that file. **Settings → Column types** sets the default date order.

Text columns have **Text ▸** (trim, change case, remove accents, add a prefix…) and **Extract ▸** (first characters, text before a delimiter, a regular expression match…) in their menu. Yes/no columns have **Invert** and **Keep yes / no / empty**. Date columns have **Date ▸** (year, month name, day of week…), **Start of / End of ▸**, **Age**, **Difference from another date…** and **Filter dates ▸** (last 7 days, this month, year to date…). JSON columns have **JSON ▸**: expand keys into columns, extract a value by path, items to rows. List and struct columns have the same entries where they apply.

A CSV file with a semicolon, title lines above the table or decimal commas opens with **File → Open with Options…**, which previews the rows. ParVu remembers the options for that file.

To convert a column after other steps, or with more options, use **Change Type → Convert…** in its menu. ParVu suggests a type and previews the result. It also counts the values that don't convert, and **Show rows that don't convert** lists them so you can fix them.

## Analyse without SQL

The menus say what an operation touches:

- **Rows:** filter rows by conditions (`Ctrl+Shift+L`), sort by several columns, keep first/last/random rows, drop or mark duplicates, drop rows with empty values.
- **Column:** formulas, rename, duplicate, remove, types, text and date functions, replace, fill missing values, **Set Value…**, split, merge, bin, running totals, ranks and rolling values.
- **Table:** group by (`Ctrl+G`), pivot table, unpivot, transpose, value counts, describe, join and append with other tabs.
- **View:** charts (`Ctrl+Shift+C`) and conditional formatting.

Row and column operations become steps you can undo; formatting, frozen columns and column widths only change the view.

### Conditions

Filter Rows and every "where" in ParVu use the same condition rows: a column, an operator, a value. They follow the column's type: numbers and dates get comparisons and **between**, text gets **contains**, **starts with**, regular expressions and suggestions of the column's most frequent values, yes/no columns get a yes/no choice. Dates come with a date picker.

### Change only some rows

**Replace**, **Fill missing values** and the column functions that open a dialog (add a prefix, extract, format dates…) have **Only rows where…**: the change applies to the rows that match, the others keep their value. **Set Value…** (column menu → Clean) writes one value, or empty, into a column on the rows that match, like pandas `df.loc[condition, column] = value`. The step names the rows it touched.

### Duplicates

**Rows → Drop Duplicates…** compares the columns you tick (all of them by default) and keeps the first or last row of each group, or none of them. **Mark Duplicates…** adds a yes/no column instead: every repeat after the first, before the last, or every duplicated row. Both are also in the column menu under **Clean**.

### Table operations open a new tab

Group By, Pivot, Unpivot, Transpose, Value Counts, Describe, Join and Append make a different table, so their result opens in a **new tab** named after what happened (`sales_by_region`, `sales_pivot`…). The tab you ran them on keeps its data and steps.

- The new tab is a table of its own: edit cells, add formulas, run any operation.
- A chip by its name says where it came from (`from sales · Group By region`): click it to go to the source, or **↻** to compute the result again from the source's current data.
- It's not a file yet: it shows the unsaved dot, `Ctrl+S` opens Save As, and closing it asks first.
- A result over 5,000,000 rows isn't kept in memory: ParVu offers to write it straight to a Parquet or CSV file and open that.

### Formula columns

**Column → New Formula Column**, the **Formula** button or the **+** header after the last column adds a live column computed from each row, such as `[qty] * [price]` or `[price] / SUM([price])`. The formula bar opens above the grid while you write it, with autocomplete; click a column header to insert it. Formulas update when cells change and are remembered for the file. Typing `=` in a cell is just a value. See [DSL.md](DSL.md) for the functions, including about 100 Excel ones (`XLOOKUP`, `COUNTIF`, `TEXT`…).

### Custom functions and operations

Add your own tools to the menus: a function such as `clean_phone(phone)` appears under **Column → Custom functions** for text columns, and an operation such as `top_n_per_group` under **Rows** (it changes the table) or **Table** (it opens a new one). You write the body in SQL, declare the parameters' types, and give examples that must pass before ParVu saves it. **Table → Manage custom tools…** lists them, with import and export to share them. ParVu comes with seven samples. See [CUSTOM.md](CUSTOM.md).

### Charts

Select a column and press `Ctrl+Shift+C`: ParVu suggests a chart (a date → a line over time, a number → its distribution, text → a bar per value). Then:

- **Group by / Value:** what goes along the axis and how it is calculated (sum, mean, count…, or just the number of rows).
- **Split by:** one series per value of a category, stacked or side by side; past seven values the rest becomes a gray "Other".
- **Group dates by:** day, week, month, quarter or year (automatic picks one from the date range).
- **Show:** the top N bars, the rest grouped as "Other", sorted by value or by label.

Hover for exact values, tick **Show the numbers as a table** to read them, and **Save image…** to export PNG or SVG. The chart covers the tab's current result, so filter or group first to chart a subset.

## Edit and save

While no transform is applied, you can type into any cell. Edited cells are highlighted, and values that don't fit the column type are rejected.

- `Ctrl+D` copies the top value of each selected column down the selection, and `Ctrl+R` the left value of each row to the right (one undoable step each). **Edit → Fill** also fills up and left.
- Drag the small square at the corner of a selection to fill the next cells with a series: `1, 2` → `3, 4`, dates by day or month, `Jan` → `Feb`, `Item 1` → `Item 2`. Anything else is repeated.
- **Edit → Fill → Series…** fills the selection from its first cell with a step you choose: add it, multiply by it, or step dates by days, weeks, months or years, optionally up to a stop value.
- `Ctrl+S` saves edits and transforms back into the file (a result tab asks where to save it).
- File → Save As (`Ctrl+Shift+S`) writes a new Parquet, CSV, JSON or Excel file.
- File → Export (`Ctrl+E`) writes the current result with options: columns, types, date formats, delimiter, compression.

## Language and theme

- View → Language: English, Russian or Azerbaijani.
- View → Theme: Material Light (default), Material Dark, ParVu Light, Excel Material, Jupyter, ParVu Black, or your own.

Both apply immediately.

## Shortcuts

Press `Ctrl+/` for the full list.

| Keys | Action |
|---|---|
| `Ctrl+O` | Open file |
| `Ctrl+N` | New window |
| `Ctrl+Enter` | Run the query |
| `Ctrl+Z` | Undo the last step |
| `Ctrl+S` / `Ctrl+Shift+S` | Save / Save As |
| `Ctrl+E` | Export |
| `Ctrl+F`, `F3`, `Shift+F3` | Search, next, previous |
| `Ctrl+Shift+L` | Filter rows |
| `Ctrl+G` | Group by |
| `Ctrl+Shift+C` | Chart |
| `Ctrl+D` | Fill down |
| `Ctrl+R` | Fill right |
| `Ctrl+Shift+A` | AI assistant panel |
| `Ctrl+W` | Close tab |
| `Shift+Wheel` | Scroll sideways |

## Ask the AI assistant

**AI → AI providers…** adds a model: Anthropic, OpenAI, Gemini, OpenRouter or Groq with your API key, or a local Ollama / LM Studio model. Then **AI → AI assistant** (`Ctrl+Shift+A`) opens a panel where you ask about the active table. The assistant runs queries itself; changes it proposes appear as cards you apply or reject. See [AI.md](AI.md).

## Use it from an AI client

**AI → Connect an AI client** lets Claude Desktop or Claude Code, with your own subscription, work with the tables in this window. Switch it on, then add ParVu to the client once:

```bash
claude mcp add -s user parvu-app -- parvu mcp --attach
```

The client can query freely. Every change it proposes appears as a card with a preview, and you apply or reject it. See [MCP.md](MCP.md).

## Big files

ParVu reads files in place and loads only the visible page. Long work (unique values, statistics, charts, search, table operations) runs in the background with progress and **Stop**. Queries use at most the memory limit in **Settings** (1 GB by default) and spill to disk beyond it; if ParVu's memory still grows past a safe ceiling, it stops the running queries with "This operation needed more memory than allowed" instead of slowing down the whole computer.

## Problems

Logs are in `~/.ParVu/logs/`. See [CRASH_REPORTING.md](CRASH_REPORTING.md).
