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
- Right-click a header for column actions: statistics, unique values, rename, change type, and more.
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

## Edit and save

While no transform is applied, you can type into any cell. Edited cells are highlighted, and values that don't fit the column type are rejected.

- `Ctrl+S` saves edits and transforms back into the file.
- File → Save As (`Ctrl+Shift+S`) writes a new Parquet, CSV, JSON or Excel file.
- File → Export (`Ctrl+E`) writes the current result with options: columns, types, date formats, delimiter, compression.

## Language and theme

- View → Language: English, Russian or Azerbaijani.
- View → Theme: ParVu Light, Excel, ParVu Black, or your own.

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
| `Ctrl+W` | Close tab |
| `Shift+Wheel` | Scroll sideways |

## Problems

Logs are in `~/.ParVu/logs/`. See [CRASH_REPORTING.md](CRASH_REPORTING.md).
