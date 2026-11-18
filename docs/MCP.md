# Using ParVu from AI clients (MCP)

ParVu works with AI clients that speak the [Model Context Protocol](https://modelcontextprotocol.io): Claude Desktop, Claude Code, VS Code and others. ParVu needs no API key, because the client brings its own model, for example your Claude subscription. There are two ways to connect:

| | Connect to the ParVu window | Standalone server |
|---|---|---|
| Command | `parvu mcp --attach` | `parvu mcp --allow-dir <folder>` |
| Tables | The tabs open in your ParVu window, with your filters and steps | Files the client opens itself |
| Changes | Appear in your window after you approve them | Stay in the server's session |
| ParVu window | Started for you when needed (in the background, in the tray), with AI → Connect an AI client switched on | Not needed |

## Connect to the ParVu window

1. In ParVu, choose **AI → Connect an AI client…** and switch on **Allow AI clients to connect**. The setting is remembered.
2. Add ParVu to your client once. The dialog shows the exact commands with a **Copy** button:
   - **Claude Code:** `claude mcp add -s user parvu-app -- parvu mcp --attach`
   - **Claude Desktop:** in `claude_desktop_config.json` (Settings → Developer → Edit Config), then restart Claude Desktop:
     ```json
     { "mcpServers": { "parvu-app": { "command": "parvu", "args": ["mcp", "--attach"] } } }
     ```
3. Ask about your tables, for example "Which region had the most sales in my sales tab?" or "Keep only 2024 orders and group them by month."

The client can list, describe, sample and query the open tables freely. When it wants to change a table (`apply_operation`, `set_formula`, `undo`), ParVu shows a card with:
- the step;
- the client's reason;
- the row count before and after;
- a preview of the result and the SQL (for a formula, the formula itself).

**Apply** adds a normal step to Applied Steps, which `Ctrl+Z` undoes. **Reject** tells the client no. When the client proposes a custom function or operation (`propose_custom_tool`), ParVu asks whether to open it in the custom tool editor, prefilled with the client's definition and examples; you read it, validate it and save it, or cancel. The client can't save it. Opening and saving files stays with you: in this mode the client has no `open_file` or `export` tool.

### When ParVu isn't running

`parvu mcp --attach` doesn't open ParVu when the client starts. Until the AI first uses a tool, it answers the client's startup itself (`server/discover`, `initialize`, the tool list, and "method not found" for features ParVu doesn't have), with the same answers the window gives. Starting Claude Code, or opening a new window of it, therefore doesn't open ParVu, and the order you start things in doesn't matter.

At the first tool call the bridge starts ParVu **in the background**: the window stays hidden and only ParVu's icon appears in the tray (the top bar on Ubuntu, the notification area on Windows), without taking the focus from what you're doing. Where there is no tray (see [The tray icon](#the-tray-icon)) it starts minimised in the taskbar instead. If several AI sessions call a tool at the same moment, one of them starts ParVu and the others wait for it (a lock file, `~/.ParVu/run/launch.lock`), so you get one ParVu, not one per session.

- **Approvals:** when the AI wants to change a table or proposes a custom tool while the window is hidden in the tray, ParVu shows it minimised, because a card in a hidden window would never be seen. While it is minimised or behind other windows, ParVu asks the system for your attention (the taskbar entry flashes or is marked urgent) but doesn't jump in front of your work. Open it to answer the card.
- **Closing by itself:** a background ParVu you never opened closes 10 minutes after the last AI client disconnects. Once you bring its window up (from the tray, by starting ParVu again, or by opening a file in it), it is a normal ParVu that stays until you quit it. Closing its window into the tray doesn't change that: it stays for your AI clients until you choose **Quit ParVu**.
- **AI clients switched off:** the bridge doesn't start ParVu; the tool call answers that AI clients are switched off and how to switch them on. If ParVu is open with AI clients switched off, the client is told the same. Your setting is never changed for you.
- **ParVu closes while a client is connected:** requests waiting for it get an error, and the next tool call starts or reconnects ParVu. The client doesn't have to reconnect.

Only one ParVu runs per user profile. Starting ParVu again (from the menu, a file manager or `parvu file.csv`) brings the running window to the front and opens the files there instead of starting a second copy. `parvu mcp` servers are separate processes and aren't affected.

`parvu --background` starts ParVu the way the bridge does. With AI clients switched off it opens a normal window instead, since a hidden ParVu would serve nothing. `parvu --quit` asks a running ParVu to quit, as **Quit ParVu** in the tray does (it does nothing when ParVu isn't running).

### The tray icon

While ParVu runs it shows an icon in the tray: in the top bar on Ubuntu and other GNOME desktops with the AppIndicator extension (Ubuntu enables it), in the panel on KDE, Xfce and others, in the notification area on Windows. Its menu:

- **Open ParVu** brings the window back (on Windows a left click on the icon does the same);
- a status line: **AI clients: on, 2 connected** or **AI clients: off**;
- **Quit ParVu**.

**Closing the window keeps ParVu running.** While AI clients are switched on, closing the last ParVu window hides it into the tray instead of quitting, so your AI clients keep working with the same tabs. Nothing is lost: the tabs, their steps and unsaved cell edits stay as they were, and **Open ParVu** shows them again. The first time, ParVu tells you where it went. To turn this off, clear **Settings → General → Exit Behavior → Keep running in the tray when the window is closed**; with it off, or with AI clients off, closing quits as before. **File → Exit** always closes the window. With several windows open, the others close as usual; only the last one hides.

**Quit ParVu** quits at once when nothing would be lost. When a window has unsaved edits, tables not saved to a file, or applied steps (if you asked to be warned about those), ParVu shows that window and asks, as closing it would; **Cancel** keeps ParVu running.

**Without a tray**, a hidden window could never be found again, so ParVu doesn't hide: closing quits, and background starts are minimised. On Linux ParVu checks for a tray (a StatusNotifier host) when you close the window. GNOME without the AppIndicator extension has none; install and enable it (`gnome-shell-extension-appindicator`) to get the icon. The icon also needs `libayatana-appindicator3-1`, which the ParVu package depends on.

While a client is connected, the status bar shows **AI clients: N**. With several ParVu windows open, the client works on the window you used last. Switching the setting off disconnects every client.

How it works: ParVu listens on `127.0.0.1` on a random port and accepts only connections that present a random token. The port and token are in `~/.ParVu/run/mcp.json`, which only your user account can read. `parvu mcp --attach` reads that file and relays the client's messages, so nothing is reachable from the network.

## Standalone server

`parvu mcp` runs ParVu as a server without a window. The client starts it and uses its tools to open your data files and query them with DuckDB.

```
parvu mcp [--allow-dir DIR]... [--allow-write] [FILE]...
```

| Option | Meaning |
|---|---|
| `--allow-dir DIR` | A folder the client may open files from. Repeat for several folders. Without it, the client can only use the files named on the command line. |
| `--allow-write` | Lets the `export` tool write files, only inside the allowed folders and never over an existing file unless asked. Off by default. |
| `FILE` | Files to open at start. |

### Setting up a client

The examples allow `~/data`; use your own folders, which must exist. With the beta package, the command is `parvu-next`.

**Claude Code**

```bash
claude mcp add -s user parvu -- parvu mcp --allow-dir ~/data
claude mcp get parvu        # should say ✔ Connected
```

`-s user` makes ParVu available in every folder. Without it, Claude Code registers the server for the current project only. Restart Claude Code, or reconnect with `/mcp`, after changing the configuration.

**Claude Desktop**: in `claude_desktop_config.json` (Settings → Developer → Edit Config):

```json
{
  "mcpServers": {
    "parvu": { "command": "parvu", "args": ["mcp", "--allow-dir", "/home/me/data"] }
  }
}
```

**VS Code**: in `.vscode/mcp.json`:

```json
{
  "servers": {
    "parvu": { "type": "stdio", "command": "parvu", "args": ["mcp", "--allow-dir", "${workspaceFolder}"] }
  }
}
```

Then ask, for example: "Open sales.parquet from my data folder and show revenue by region for 2024."

### Tools

| Tool | What it does |
|---|---|
| `list_tables` | Open tables with row and column counts |
| `open_file` | Open a Parquet, CSV, TSV, JSON / JSON Lines or Excel file from an allowed folder; for a workbook, optionally a `sheet` (the result lists its sheets) |
| `describe_table` | Columns and types, row count, applied steps |
| `sample_rows` | First, last or random rows |
| `column_stats` | Rows, nulls, distinct values, min, max, mean, standard deviation |
| `value_counts` | Most frequent values of a column |
| `query` | One DuckDB `SELECT` over the open tables, by table name (up to 1,000 rows, 30-second limit). The user's custom functions can be called by name, as in SQL mode ([CUSTOM.md](CUSTOM.md#in-sql-mode)) |
| `apply_operation` | Any ParVu table operation (filter, sort, add column, fill missing values, window functions…) as a step. Group by, pivot, melt, transpose, value counts, describe, join and append leave the table as it is and open their result as a new table (`new_table` in the answer) |
| `set_formula` | Add or change a formula column: a live column computed from each row, e.g. `[qty] * [price]`, `[price] / SUM([price])` (share of the column's total) or `SUM([q1:q4])` (across a row). Cells stay editable and the column updates when they change. Formulas approved in your window are remembered for the file, like your own. |
| `undo` | Revert the last step on a table |
| `list_custom_tools` | The custom functions and operations the user defined ([CUSTOM.md](CUSTOM.md)): name, kind (row function, summary function, operation), typed signature, description, one example and how to call each one with `apply_operation` (`custom_function` / `custom_operation`) or `set_formula` (a summary function there is a column total, like `SUM`). A columns parameter takes `{"columns": ["a", "b"]}`; a summary function of one column also shows the `group_by` aggregation that uses it (`"func": "custom"`) |
| `propose_custom_tool` | Suggest a new custom function or operation: a full definition with examples. ParVu validates it with every check (including the safety guard that refuses file functions) and returns the report in English. It never saves anything: connected to the window, the user is asked whether to open the proposal in the editor and saves it there; the standalone server only validates, even with `--allow-write`. `offer: false` validates without asking the user (for corrections) |
| `export` | Write a table to a file (needs `--allow-write`) |

### Safety

- **Files:** the client can open files only from the allowed folders. Paths are resolved first, so `..` and symbolic links can't escape them.
- **Queries:** `query` runs one `SELECT` and reads only the open tables. Statements that change anything (`INSERT`, `COPY`, `ATTACH`, `PRAGMA`, …) are refused. So are DuckDB functions that read files (`read_csv`, `read_text`, `glob`, …) and file paths used as table names, so a query can't reach files you didn't allow. The same check applies to expressions inside `apply_operation`. Calls of custom functions are expanded into their SQL **before** the check, so it sees everything that runs, arguments included.
- **Changes:** `apply_operation` and `undo` change only this session's tables, never your files. Only `export` writes, and only with `--allow-write`.
- **Approval:** your AI client decides whether tool calls need your approval; Claude Code and Claude Desktop ask by default.
- **What the model sees:** query results go to the client's model. Treat it like pasting those rows into a chat with that model.

Logs go to ParVu's usual log folder (`~/.ParVu/logs`), never to the protocol stream.

## Troubleshooting

**`parvu mcp --attach`: "ParVu is open, but AI clients are not allowed".** In ParVu, switch on AI → Connect an AI client, then reconnect the server in your client (Claude Code: `/mcp`; Claude Desktop: restart it).

**"AI clients are switched off in ParVu, so it was not started".** Open ParVu, switch on AI → Connect an AI client, then ask again; the client doesn't have to reconnect.

**"ParVu did not start in time".** The bridge starts ParVu when it isn't running and waits up to 25 seconds. On a slow first start, open ParVu yourself and ask again.

**ParVu's icon appeared in the top bar (or a minimised ParVu in the taskbar).** An AI session used a ParVu tool. It closes by itself 10 minutes after the last AI client disconnects, unless you opened it (`PARVU_BACKGROUND_IDLE_SECS` in the client's environment changes the delay).

**I closed ParVu but it is still running.** AI clients are on, so ParVu stays in the tray for them: choose **Quit ParVu** in the icon's menu, run `parvu --quit`, or turn off **Keep running in the tray when the window is closed** in Settings.

**I closed ParVu and can't see its icon.** Your desktop has no tray: on GNOME, enable the AppIndicator extension. ParVu doesn't hide its window when it finds no tray, so this means the tray went away after ParVu checked; start ParVu again to bring the window back, or run `parvu --quit`.

**The approval card says "An AI client".** The client didn't send its name; the change is the same.

**"Failed to connect" / "Connection closed".** Run the same command by hand to see why:

```bash
parvu mcp --allow-dir ~/data < /dev/null
```

The most common cause is an `--allow-dir` folder that doesn't exist (`--allow-dir /home/me/data: No such file or directory`). Fix the folder, then in Claude Code:

```bash
claude mcp remove parvu -s user      # or -s local, as `claude mcp get parvu` shows
claude mcp add -s user parvu -- parvu mcp --allow-dir ~/Documents
```

**"MCP server parvu already exists".** A server with that name is already registered. `claude mcp get parvu` shows its folders and scope; remove it first as above.

**A ParVu window opens instead of a server, or the client times out.** The installed ParVu is older than the MCP server; `parvu mcp --help` prints the usage only on versions that have it. Install a newer build (`./install.sh` from a checkout of `main`, or a release that lists MCP in its notes).

**A file "is outside the allowed folders".** Add that folder with another `--allow-dir`, or move the file into an allowed one.

Logs are in `~/.ParVu/logs`; start the server with `PARVU_LOG=debug` in the client's environment for more detail.

## Notes

- A standalone `parvu mcp` process has its own tables. To work with the tabs in your window, use `parvu mcp --attach` (above).
- On Windows the installed program is a GUI application; MCP over stdio has not been tested there yet.
