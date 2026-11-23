# AI in ParVu

There are two ways to use AI with your tables:

| | Built-in assistant | Connect an AI client |
|---|---|---|
| Where you type | The **AI assistant** panel in ParVu | Claude Desktop, Claude Code, VS Code… |
| Paid through | An API key (Anthropic, OpenAI, Gemini, OpenRouter, Groq), or free with a local model (Ollama, LM Studio) | Your Claude subscription or the client's own account |
| Set up in | AI → AI providers… | AI → Connect an AI client… ([MCP.md](MCP.md)) |

Both use the same tools and the same safety rules. The AI can read the tables open in your window. Any change it wants to make appears as a card in ParVu, showing the step, the AI's reason, the row count before and after, and a preview. Nothing changes until you click **Apply**, and every applied change is an ordinary step that `Ctrl+Z` undoes.

When you connect an AI client, ParVu doesn't have to be open: the client starts it in the background (only its icon in the tray) when it first needs it, and that ParVu closes by itself once the clients are gone, unless you opened it. While AI clients are on, closing the ParVu window keeps ParVu running in the tray; quit it from the icon's menu. See [MCP.md](MCP.md#when-parvu-isnt-running) and [the tray icon](MCP.md#the-tray-icon).

## The built-in assistant

1. **Add a provider:** **AI → AI providers…** → choose a provider type → **Add** → paste your API key → **Save and test**.
   - **Anthropic (Claude):** suggests `claude-opus-5-5`, `claude-sonnet-5-5` (cheaper) and `claude-haiku-5-5` (cheapest).
   - **OpenAI, Gemini, OpenRouter, Groq:** enter the model id your account offers.
   - **Ollama / LM Studio:** start the local server, enter a model you've pulled (for example `llama3.1`). No key is needed and nothing leaves your computer. Small local models may not handle tool calls well.
2. **Open the panel:** **AI → AI assistant** (`Ctrl+Shift+A`).
3. **Ask:** for example "Which region sold the most last quarter?", "Are there duplicate customer ids?", "Group by month and sum revenue." or "Add a column with each order's share of total revenue." (a live formula column, see [DSL.md](DSL.md#column-formulas)) `Enter` sends, `Shift+Enter` starts a new line, **Stop** cancels.

The assistant also sees your custom functions and operations (`list_custom_tools`) and can use them. Ask it to "make a custom function that …" and it proposes one with examples: ParVu checks it, and asks whether to open it in the custom tool editor for you to read, validate and save. The assistant never saves a tool itself. See [CUSTOM.md](CUSTOM.md#ai-clients-and-the-assistant).

### Keeping what the assistant ran

Each tool call the assistant makes is a row in its answer; click it to see its full input and result. A query shows its SQL highlighted, and its rows as a small table.

- **Copy SQL** copies the query.
- **Open in new tab** runs the query again, without the 100–1000 row limit the assistant works with, and opens the result as a new tab named `ai_result` (then `ai_result_2`…). Save it with **Save As** or **Export** like any other result. The tab's ↻ button runs the query again on your tabs as they are then. `value_counts` and `sample_rows` results can be opened the same way (value counts then lists every value, not only the top ones).
- **Edit in SQL editor** puts the query into the SQL editor of the current tab without running it.
- **Copy result** copies the rows as tab-separated text, which pastes into a spreadsheet. `describe_table` and `column_stats` results can be copied too.

SQL code blocks in an answer get the same **Copy SQL**, **Open in new tab** and **Edit in SQL editor** buttons; other code blocks get **Copy**. The copy button under an answer copies its whole text.

**Why Edit in SQL editor is sometimes greyed out:** in the assistant's queries a table name means the tab *as you see it*, with its filters and other steps applied. In the SQL editor a table name means the original file. When a query names a tab that has steps, the two would give different numbers, so ParVu offers only **Open in new tab** for it, which keeps the assistant's meaning. The button's tooltip says which table has steps. (A column with the same name as such a tab also counts, to be safe.)

Queries opened in a new tab are checked like the assistant's own: one SELECT over the open tabs, with no file functions such as `read_csv`.

Each tab has its own conversation. The ↺ button starts a new one. The panel shows the tokens used and, for Claude models, the estimated cost.

### Conversations are remembered per file

When you open a file again (from File → Open, the recent files, or the tabs restored at startup), the panel shows its earlier conversation under a line **Earlier conversation · date**, and the assistant remembers it: a follow-up question such as "and by month?" carries on from there. An Excel workbook has one conversation per sheet. Several tabs on the same file share it (the one that finished last is saved).

- A result tab (group by, pivot, `ai_result`…) has no conversation saved until you save it to a file with **Save As**; its conversation then goes with the new file.
- The ↺ button (New conversation) also deletes the saved one.
- Only the latest 50 questions of a file are kept, and about 1 MB; older questions are dropped first.
- To stop saving, turn off **Settings → General → AI Assistant → Remember AI conversations per file**. ParVu then asks whether to delete the conversations already saved. **Delete all saved conversations** there deletes them at any time.

**Where they are kept:** in `~/.ParVu/ai/chats`, one file per data file, on your computer only. They contain your questions, the answers and the results of the assistant's queries, so they hold excerpts of your data. They are never sent anywhere, except that a remembered conversation goes to your provider with your next question about that file, as within a session.

**What is sent to the provider:**
- your question;
- the active table's name, columns and first three rows;
- the names of the other open tabs;
- the results of the queries the assistant runs.

Whole files are never sent.

**Claude settings:** on an Anthropic provider, **Effort** trades depth for speed and cost (Medium is the default). **If Claude declines a request, retry it on Anthropic's fallback model** is on by default: when Claude's safety checks decline a request, Anthropic retries it on another model within the same call.

## API keys

- **Storage:** keys are stored in your system keychain (GNOME Keyring or KWallet on Linux, Credential Manager on Windows, Keychain on macOS) under the service `ParVu`. They are never written to ParVu's files, and the dialog never shows them again: type a new one to replace it, or click **Forget key**.
- **Without a keychain:** set the provider's **Key variable** (for example `ANTHROPIC_API_KEY`) and export that variable before starting ParVu.
- **Provider list:** kept in `~/.ParVu/ai/providers.json`, which contains no keys.
- **Logs and errors:** keys are removed from logs and error messages.

## Troubleshooting

- **"No API key":** add the key in AI providers, or export the variable named there.
- **"error (401)":** the key is wrong or revoked.
- **"error (429)":** you hit the provider's rate limit. ParVu retries twice, then shows the error.
- **A local model answers but never uses the tools:** that model doesn't support tool calls well. Try a larger model or a cloud provider.
- **"Stopped after too many tool calls":** ask a narrower question, or ask it to continue.
