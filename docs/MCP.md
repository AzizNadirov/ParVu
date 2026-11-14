# Using ParVu from AI clients (MCP)

`parvu mcp` runs ParVu as a [Model Context Protocol](https://modelcontextprotocol.io) server. AI clients such as Claude Code, Claude Desktop or VS Code start it and use its tools to open your data files and query them with DuckDB. No window opens, and ParVu needs no API key: the client brings its own model.

```
parvu mcp [--allow-dir DIR]... [--allow-write] [FILE]...
```

| Option | Meaning |
|---|---|
| `--allow-dir DIR` | A folder the client may open files from. Repeat for several folders. Without it, the client can only use the files named on the command line. |
| `--allow-write` | Lets the `export` tool write files, only inside the allowed folders and never over an existing file unless asked. Off by default. |
| `FILE` | Files to open at start. |

## Setting up a client

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

## Tools

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
| `undo` | Revert the last step on a table |
| `export` | Write a table to a file (needs `--allow-write`) |

## Safety

- **Files:** the client can open files only from the allowed folders. Paths are resolved first, so `..` and symbolic links can't escape them.
- **Queries:** `query` runs one `SELECT` and reads only the open tables. Statements that change anything (`INSERT`, `COPY`, `ATTACH`, `PRAGMA`, …) are refused. So are DuckDB functions that read files (`read_csv`, `read_text`, `glob`, …) and file paths used as table names, so a query can't reach files you didn't allow. The same check applies to expressions inside `apply_operation`.
- **Changes:** `apply_operation` and `undo` change only this session's tables, never your files. Only `export` writes, and only with `--allow-write`.
- **Approval:** your AI client decides whether tool calls need your approval; Claude Code and Claude Desktop ask by default.
- **What the model sees:** query results go to the client's model. Treat it like pasting those rows into a chat with that model.

Logs go to ParVu's usual log folder (`~/.ParVu/logs`), never to the protocol stream.

## Troubleshooting

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

- Each `parvu mcp` process has its own tables; it does not see the tabs of a running ParVu window. Connecting to the running app is planned (`plans/AI_PLAN.md` §19).
- On Windows the installed program is a GUI application; MCP over stdio has not been tested there yet.
