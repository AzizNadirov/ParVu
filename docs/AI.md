# AI in ParVu

There are two ways to use AI with your tables:

| | Built-in assistant | Connect an AI client |
|---|---|---|
| Where you type | The **AI assistant** panel in ParVu | Claude Desktop, Claude Code, VS Code… |
| Paid through | An API key (Anthropic, OpenAI, Gemini, OpenRouter, Groq), or free with a local model (Ollama, LM Studio) | Your Claude subscription or the client's own account |
| Set up in | AI → AI providers… | AI → Connect an AI client… ([MCP.md](MCP.md)) |

Both use the same tools and the same safety rules. The AI can read the tables open in your window. Any change it wants to make appears as a card in ParVu, showing the step, the AI's reason, the row count before and after, and a preview. Nothing changes until you click **Apply**, and every applied change is an ordinary step that `Ctrl+Z` undoes.

## The built-in assistant

1. **Add a provider:** **AI → AI providers…** → choose a provider type → **Add** → paste your API key → **Save and test**.
   - **Anthropic (Claude):** suggests `claude-opus-5-5`, `claude-sonnet-5-5` (cheaper) and `claude-haiku-5-5` (cheapest).
   - **OpenAI, Gemini, OpenRouter, Groq:** enter the model id your account offers.
   - **Ollama / LM Studio:** start the local server, enter a model you've pulled (for example `llama3.1`). No key is needed and nothing leaves your computer. Small local models may not handle tool calls well.
2. **Open the panel:** **AI → AI assistant** (`Ctrl+Shift+A`).
3. **Ask:** for example "Which region sold the most last quarter?", "Are there duplicate customer ids?", "Group by month and sum revenue." or "Add a column with each order's share of total revenue." (a live formula column, see [DSL.md](DSL.md#column-formulas)) `Enter` sends, `Shift+Enter` starts a new line, **Stop** cancels.

Each tab has its own conversation. The ↺ button starts a new one. The panel shows the tokens used and, for Claude models, the estimated cost.

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
