# Using ParVu from AI clients (MCP)

ParVu works with AI clients that speak the [Model Context Protocol](https://modelcontextprotocol.io): Claude Desktop, Claude Code, VS Code and others. ParVu needs no API key, because the client brings its own model, for example your Claude subscription. There are two ways to connect:

| | Connect to the ParVu window | Standalone server |
|---|---|---|
| Command | `parvu mcp --attach` | `parvu mcp --allow-dir <folder>` |
| Tables | The tabs open in your ParVu window, with your filters and steps | Files the client opens itself |
| Changes | Appear in your window after you approve them | Stay in the server's session |
| ParVu window | Must be open, with AI → Connect an AI client switched on | Not needed |

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

**Apply** adds a normal step to Applied Steps, which `Ctrl+Z` undoes. **Reject** tells the client no. Opening and saving files stays with you: in this mode the client has no `open_file` or `export` tool.

If ParVu isn't running, `parvu mcp --attach` waits: it answers the client's startup itself, and opens ParVu only when the AI first uses a tool. Starting Claude Code (or a new VS Code window) therefore doesn't open ParVu, and the order you start things in doesn't matter. If ParVu is open with AI clients switched off, the client is told how to switch them on; no second window opens.

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
| `open_file` | Open a Parquet, CSV, TSV, JSON / JSON Lines or Excel file from an allowed folder |
| `describe_table` | Columns and types, row count, applied steps |
| `sample_rows` | First, last or random rows |
| `column_stats` | Rows, nulls, distinct values, min, max, mean, standard deviation |
| `value_counts` | Most frequent values of a column |
| `query` | One DuckDB `SELECT` over the open tables, by table name (up to 1,000 rows, 30-second limit) |
| `apply_operation` | Any ParVu table operation (filter, sort, group by, pivot, melt, add column, fill missing values, window functions…) as a step |
| `set_formula` | Add or change a formula column: a live column computed from each row, e.g. `[qty] * [price]`, `[price] / SUM([price])` (share of the column's total) or `SUM([q1:q4])` (across a row). Cells stay editable and the column updates when they change. Formulas approved in your window are remembered for the file, like your own. |
| `undo` | Revert the last step on a table |
| `export` | Write a table to a file (needs `--allow-write`) |

### Safety

- **Files:** the client can open files only from the allowed folders. Paths are resolved first, so `..` and symbolic links can't escape them.
- **Queries:** `query` runs one `SELECT` and reads only the open tables. Statements that change anything (`INSERT`, `COPY`, `ATTACH`, `PRAGMA`, …) are refused. So are DuckDB functions that read files (`read_csv`, `read_text`, `glob`, …) and file paths used as table names, so a query can't reach files you didn't allow. The same check applies to expressions inside `apply_operation`.
- **Changes:** `apply_operation` and `undo` change only this session's tables, never your files. Only `export` writes, and only with `--allow-write`.
- **Approval:** your AI client decides whether tool calls need your approval; Claude Code and Claude Desktop ask by default.
- **What the model sees:** query results go to the client's model. Treat it like pasting those rows into a chat with that model.

Logs go to ParVu's usual log folder (`~/.ParVu/logs`), never to the protocol stream.

## Troubleshooting

**`parvu mcp --attach`: "ParVu is open, but AI clients are not allowed".** In ParVu, switch on AI → Connect an AI client, then reconnect the server in your client (Claude Code: `/mcp`; Claude Desktop: restart it).

**"ParVu did not start in time".** The bridge opens ParVu when it isn't running and waits up to 25 seconds. On a slow first start, open ParVu yourself and reconnect.

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
