# ParVu Quick Start

## Open a file

- Drop one or more files onto the window, or
- File → Open (`Ctrl+O`), or
- paste a path on the start screen and press Enter, or
- run `parvu file.parquet other.csv`.

Each file opens in its own tab at the bottom of the window. The table is named after the file: `sales 2024.csv` becomes `sales_2024`.

## Look around

- Move between pages with the pagination bar, or type a page number.
- Click a column header to sort; click again to reverse.
- Right-click a header for column actions: filter, value counts, chart, statistics, unique values, rename, change type, freeze, conditional formatting, and more.
- `Ctrl+F` searches every row on every page.

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

## Column types

Each column header shows an icon for its type: numbers, text, yes/no, dates, times, JSON. For CSV files, right-click a header and choose **Read as** to read the column another way, for example a number written `1.234,56`, or a date written `31.12.2024`. A calendar icon with a question mark means the day and month order was a guess; **Read dates day first / month first** in the same menu flips it. ParVu remembers your choice for that file. **Settings → Column types** sets the default date order.

Text columns have **Text ▸** (trim, change case, remove accents, add a prefix…) and **Extract ▸** (first characters, text before a delimiter, a regular expression match…) in their menu. Yes/no columns have **Invert** and **Keep yes / no / empty**.

To convert a column after other steps, or with more options, use **Change Type → Convert…** in its menu. ParVu suggests a type and previews the result. It also counts the values that don't convert, and **Show rows that don't convert** lists them so you can fix them.

## Analyse without SQL

- **Data** menu: filter rows by conditions (`Ctrl+Shift+L`), fill missing values, sort by several columns, choose and reorder columns, keep first/last/random rows, split a column, bin values.
- **Reshape** menu: group by (`Ctrl+G`), pivot table, unpivot, transpose, value counts, describe.
- **Operations** menu: running totals, ranks, shifts and rolling values (per group if you like), math operations, join and append with other tabs.
- **View** menu: charts (`Ctrl+Shift+C`) and conditional formatting.

Each one becomes a step you can undo; formatting, frozen columns and column widths only change the view.

### Charts

Select a column and press `Ctrl+Shift+C`: ParVu suggests a chart (a date → a line over time, a number → its distribution, text → a bar per value). Then:

- **Group by / Value:** what goes along the axis and how it is calculated (sum, mean, count…, or just the number of rows).
- **Split by:** one series per value of a category, stacked or side by side; past seven values the rest becomes a gray "Other".
- **Group dates by:** day, week, month, quarter or year (automatic picks one from the date range).
- **Show:** the top N bars, the rest grouped as "Other", sorted by value or by label.

Hover for exact values, tick **Show the numbers as a table** to read them, and **Save image…** to export PNG or SVG. The chart covers the tab's current result, so filter or group first to chart a subset.

## Edit and save

While no transform is applied, you can type into any cell. Edited cells are highlighted, and values that don't fit the column type are rejected.

- `Ctrl+D` copies the top value of each selected column down the selection (one undoable step).
- `Ctrl+S` saves edits and transforms back into the file.
- File → Save As (`Ctrl+Shift+S`) writes a new Parquet, CSV, JSON or Excel file.
- File → Export (`Ctrl+E`) writes the current result with options: columns, types, date formats, delimiter, compression.

## Language and theme

- View → Language: English, Russian or Azerbaijani.
- View → Theme: Material Light (default), Material Dark, ParVu Light, Excel, ParVu Black, or your own.

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

## Problems

Logs are in `~/.ParVu/logs/`. See [CRASH_REPORTING.md](CRASH_REPORTING.md).
