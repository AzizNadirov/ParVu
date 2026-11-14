# ParVu AI Assistant — Plan

> **Status:** proposal, not implemented.
> **Revised:** 2026-11-14 for the Rust app (Tauri, React, DuckDB). The earlier version, written for the Python/PyQt6 app, is in git history (`git log -p plans/AI_PLAN.md`).
> **Target:** a `1.1.0` preview after the `1.0.0` release.
> **Progress:** milestone A is done (`parvu-core/src/readonly.rs`, `Workspace::preview_op`, the `schema` feature). E1 and E2 (the `parvu mcp` stdio server) are done: `parvu-app/src/mcp.rs`, `docs/MCP.md`.

The assistant lets people ask questions about their data in plain language and get either an answer, which it works out by inspecting the data with read-only tools, or a proposed change to the table, which the user approves before it runs. It never changes data on its own.

---

## 0. What changed from the Python plan

| Topic | Python plan | This plan |
|---|---|---|
| Agent framework | Pydantic AI | A small agent loop of our own in a new `parvu-ai` crate. There is no official Anthropic SDK for Rust, and the loop needs approval gates, cancellation and redaction that we want to control. |
| "Capability layer" refactor | Milestone A: two weeks of moving every operation into a `Capability` registry | Mostly not needed. Every table operation is already one serde-tagged `Operation` enum value with a step label, undo and tests. Tools are generated from it with `schemars`. |
| Read-only SQL check | `sqlglot` AST | DuckDB's own parser (`json_serialize_sql`), plus a check of every table reference, so the assistant cannot read files that aren't open in a tab (§6). |
| Threading | `QThread` plus an asyncio loop | Tauri's tokio runtime for HTTP; DuckDB work on the blocking pool. LLM calls never hold the workspace lock (§5). |
| Cancellation | "DuckDB queries can't be interrupted" | DuckDB's `InterruptHandle` stops a running query, as the export dialog already does. |
| UI | `QDockWidget` and `QFrame` cards | A React side panel, components in `ui/src/components/ai/`, Material styling and `t()` strings |
| Credentials | `keyring` package plus a Fernet file | `keyring` crate (Secret Service, macOS Keychain, Windows Credential Manager), with an environment-variable fallback (§8) |

The goals, UX, approval flow, privacy rules and context strategy carry over largely unchanged.

---

## 1. Goals and non-goals

### Goals
- **Answer questions about the data.** "What's the average price by category?" The assistant inspects the data with read-only tools and answers, showing the SQL it used.
- **Propose changes, never make them.** Filters, group-bys, new columns and so on come back as a proposal card. Approving it applies a normal Applied Step, so undo works as usual.
- **Many providers.** Anthropic (native API), plus anything that speaks the OpenAI Chat Completions format: OpenAI, OpenRouter, Groq, Google Gemini (through its OpenAI-compatible endpoint), and local servers (Ollama, LM Studio, llama.cpp).
- **Local-first secrets.** API keys live in the OS keychain. They never appear in settings files, logs, crash reports or exported transcripts.
- **Visible cost and activity.** An activity strip shows tool calls, token counts, estimated cost and a Stop button.
- **Off until set up.** No network calls and no AI UI noise until the user adds a provider.

### Non-goals (first release)
- Autonomous multi-step runs that change data without the user.
- Embeddings or vector search over row contents (privacy, cost; see §9).
- Sending whole files to the provider. The assistant sees the schema, a few sample rows, and the results of the queries it runs.
- Voice input, cloud sync of chats or keys.

---

## 2. UX

### 2.1 Layout

```
┌ App bar: File  Edit  View  Data  Reshape  Operations  AI  Help ─────────────────┐
├───────────────────────────────────────────────────────────┬─────────────────────┤
│ Query editor card    Mode: [SQL | Expression | Ask AI]     │  AI panel           │
├───────────────────────────────────────────────────────────┤  (right side,       │
│ Applied Steps                                              │   resizable,        │
├───────────────────────────────────────────────────────────┤   closed by default)│
│ Data grid                                                  │                     │
├───────────────────────────────────────────────────────────┴─────────────────────┤
│ Pagination                                                                      │
├─────────────────────────────────────────────────────────────────────────────────┤
│ ▸ AI activity · 3 tool calls · 14.2k tokens · $0.02 · 1.2 s        [Stop]        │  ← only while AI is enabled
├─────────────────────────────────────────────────────────────────────────────────┤
│ Tabs / status bar                                                               │
└─────────────────────────────────────────────────────────────────────────────────┘
```

There are three ways in:

1. **AI panel**: a right-side panel (`ui/src/components/ai/AiPanel.tsx`). It is resizable, its width and open state are saved in settings, and it is closed until first use.
2. **"Ask AI" editor mode**: a third segment next to SQL and Expression. Execute (`Ctrl+Enter`) sends the editor text to the assistant and opens the panel.
3. **Activity strip**: a one-line bar shown only when AI is enabled. Click it to expand a log of events (§2.3).

The chat belongs to the window. Each tab has its own conversation, because each has its own table and steps.

### 2.2 Panel anatomy

```
┌──────────────────────────────────────────┐
│ AI · Claude Opus 5.5 ▾        ⚙    ✕    │  provider/model picker, settings
├──────────────────────────────────────────┤
│  how many nulls in price?          (you) │
│                                          │
│  ▸ column_stats(price)                   │  tool call row, expandable
│  There are 1,204 missing prices (3.1 %   │
│  of 38,712 rows).                        │
│                                          │
│ ┌ Proposed change ─────────────────────┐ │
│ │ Filter rows: price is not empty      │ │  step label, as in Applied Steps
│ │ 38,712 → 37,508 rows                 │ │  preview from DuckDB
│ │ ▸ SQL                                │ │
│ │        [Reject]  [Apply]             │ │
│ │ ☐ Apply filters automatically this   │ │
│ │   session                            │ │
│ └──────────────────────────────────────┘ │
├──────────────────────────────────────────┤
│ [ Ask about this table…            ] ➤  │
└──────────────────────────────────────────┘
```

- Answers render Markdown (we already ship `marked`), including tables and SQL blocks with a **Run in editor** button.
- Tool calls are one-line rows. Expanding a row shows its arguments and a short result preview.
- Proposals are their own card type with Apply and Reject buttons, never plain text.
- Material components from `styles.css`; colours only from `--pv-*` and `--md-*` tokens.

### 2.3 Activity log

```
14:32:08.123 agent  question in tab "sales"
14:32:08.412 llm    → claude-opus-5-5, 3,120 input tokens (2,900 cached)
14:32:09.001 tool   column_stats(column="price")
14:32:09.118 tool   ← 1 row, 117 ms
14:32:09.612 llm    ← 84 output tokens · $0.0004
14:32:09.612 agent  done in 1.5 s
```

Lines are tagged `agent`, `llm`, `tool`, `error` and coloured by theme. **Copy** produces a sanitised transcript (§12).

### 2.4 Keyboard

| Action | Shortcut |
|---|---|
| Toggle AI panel | `Ctrl+Shift+A` |
| Focus AI input | `Ctrl+L` |
| Stop the assistant | `Esc` in the panel, or Stop |
| Apply the proposal in focus | `Ctrl+Enter` on the card |

Add them to `ShortcutsDialog` and the `onKey` handler in `App.tsx`.

---

## 3. Tools

### 3.1 Read-only tools (run without asking)

These map onto `Workspace` methods that already exist or are small additions.

| Tool | Arguments | Implementation | Limits |
|---|---|---|---|
| `list_tabs` | — | `Workspace::summaries` | — |
| `describe_table` | `table?` | `columns` + `summary` (rows, steps) | — |
| `sample_rows` | `table?`, `mode: head\|tail\|random`, `n` | the `slice` SQL from `clean.rs`, read-only | n ≤ 50 |
| `column_stats` | `column`, `table?` | `Workspace::column_stats` | — |
| `value_counts` | `column`, `k`, `table?` | `value_counts` op SQL, not applied | k ≤ 50 |
| `run_sql` | `sql` | guarded read-only query (§6) | ≤ 200 rows, 20 s |

`table?` defaults to the active tab. Other tabs can be read when the user names them.

Results go back to the model as compact JSON (column names once, then rows as arrays) and are cut to a token budget. A truncation flag tells the model when to narrow the query.

### 3.2 Proposal tools (need approval)

| Tool | Input | What the user sees |
|---|---|---|
| `propose_operation` | `{ operation: Operation, rationale }` | Step label, row-count change, preview rows, generated SQL |
| `propose_query` | `{ sql, rationale }` | The SQL; applying it runs it as a `run_sql` step on the tab |
| `propose_expression` | `{ expression, rationale }` | A ParVu expression (DSL) with its compiled SQL |

- **Schema.** `propose_operation`'s input schema is generated from `parvu_core::ops::Operation` with `schemars` (a `schema` feature on `parvu-core`, so the dependency stays optional). New operations become available to the assistant automatically.
- **Descriptions.** Each variant gets a `///` doc comment, which `schemars` copies into the schema as the description the model reads.
- **Validation.** The input is deserialised into `Operation`; serde rejects unknown fields and wrong types.
- **Preview.** The tool builds a preview with a new `Workspace::preview_op(id, &op) -> OpPreview { label, sql, rows_before, rows_after, sample: Page }`. It compiles the operation against the tab's current query and runs `COUNT(*)` and a `LIMIT 20` on the result without adding a step.
- **End of turn.** A proposal ends the assistant's turn. The model does not see the outcome until the user's next message, so it cannot chain several changes before the user reacts.

### 3.3 What is *not* a tool

Saving, exporting, opening files, cell edits, closing tabs and settings. The assistant may suggest them in text; the user does them.

---

## 4. Architecture

### 4.1 Crates

```
crates/parvu-ai       NEW: providers, agent loop, tool definitions, context, pricing, redaction
                      no Tauri, no DuckDB: data access goes through a trait
crates/parvu-core     + preview_op, read-only query guard, optional `schema` feature
crates/parvu-config   + AI settings fields, providers.json, keyring access
crates/parvu-app      + ai_* commands, DataTools implementation over Workspace, event forwarding
ui/src/components/ai  NEW: AiPanel, MessageList, ToolCallRow, ProposalCard, ActivityStrip
ui/src/dialogs        + AiProvidersDialog
```

Dependencies still point down: `ui → parvu-app → (parvu-ai, parvu-core, parvu-dsl, parvu-config)`. `parvu-ai` depends on none of the other ParVu crates except through the `DataTools` trait and plain serde types, so it can be tested with a fake provider and fake data.

```rust
// parvu-ai: implemented in parvu-app over the window's Workspace.
pub trait DataTools: Send + Sync {
    fn list_tabs(&self) -> Result<Vec<TabInfo>>;
    fn describe(&self, table: &str) -> Result<TableInfo>;
    fn sample(&self, table: &str, mode: SampleMode, n: u32) -> Result<Rows>;
    fn column_stats(&self, table: &str, column: &str) -> Result<serde_json::Value>;
    fn value_counts(&self, table: &str, column: &str, k: u32) -> Result<Rows>;
    fn run_readonly(&self, sql: &str, max_rows: u32, cancel: &CancelToken) -> Result<Rows>;
    fn preview(&self, table: &str, proposal: &Proposal) -> Result<ProposalPreview>;
}
```

### 4.2 `parvu-ai` modules

```
src/lib.rs
src/provider/mod.rs        Provider trait, ProviderConfig, neutral Message / ContentBlock / ToolSpec types
src/provider/anthropic.rs  Messages API (SSE streaming, tool use, caching, thinking, refusals)
src/provider/openai.rs     Chat Completions (OpenAI, OpenRouter, Groq, Gemini compat, Ollama, LM Studio)
src/agent.rs               the loop: request → tool calls → results → … → answer or proposal
src/tools.rs               tool specs (schemas) and dispatch onto DataTools
src/context.rs             system prompt and table snapshot (§9)
src/events.rs              AgentEvent enum sent to the UI
src/pricing.rs             bundled price table + user overrides, cost per usage
src/redact.rs              key patterns, transcript sanitiser
src/fake.rs                scripted provider for tests (behind cfg(test) or a `testing` feature)
```

### 4.3 The agent loop

```
ask(question)
  build request: system prompt (stable) + tools (stable) + history + table snapshot + question
  loop (≤ 12 tool calls per question, ≤ 3 minutes):
    stream response → forward text deltas and events to the UI
    stop_reason:
      end_turn    → done
      tool_use    → run all tool calls (in parallel where read-only),
                    send every result back in ONE message (errors as is_error results)
      proposal    → emit Proposal event, end the turn
      max_tokens  → ask the model to continue once, then stop
      refusal     → show "the provider declined this request", end
  cancel at any point: abort HTTP, interrupt DuckDB, emit Cancelled
```

- **Async.** Async on tokio (Tauri's runtime). Tool calls hop to `spawn_blocking` because DuckDB is synchronous.
- **Cancellation.** A `CancelToken` aborts the HTTP stream and calls the read connection's `InterruptHandle`.
- **Events.** Sent to the window that asked, with `window.emit("ai://event", AgentEvent)`; the panel subscribes with `listen`.

### 4.4 Providers

Two wire formats cover every provider on the list. A provider-neutral message model sits between the loop and the wire.

```rust
pub struct ProviderConfig {
    pub id: String,                 // "anthropic", "ollama-laptop"
    pub kind: ProviderKind,         // Anthropic | OpenAiCompatible
    pub display_name: String,
    pub model: String,              // "claude-opus-5-5", "llama3.1:8b"
    pub base_url: Option<String>,   // required for OpenAI-compatible
    pub key_ref: Option<String>,    // name in the keychain; never the key itself
    pub max_output_tokens: u32,
    pub effort: Option<Effort>,     // Anthropic: low | medium | high
    pub request_timeout_s: u32,
}
```

**Anthropic** (`POST https://api.anthropic.com/v1/messages`, `anthropic-version: 2026-10-21`):
- **Default model:** `claude-opus-5-5`. The picker also offers `claude-sonnet-5-5` and `claude-haiku-5-5` as cheaper choices (prices in §10).
- **Effort:** set `output_config.effort` explicitly, default `medium`. Thinking can't be turned off on Opus 5.5, and effort is how depth is controlled.
- **Thinking:** adaptive thinking with `display: "summarized"`, shown collapsed under each answer. Thinking blocks are sent back unchanged on later requests.
- **Streaming:** always (`stream: true`). Tools set `eager_input_streaming: true`, and every streamed tool input is validated against its schema (serde) before it runs.
- **Tool choice:** `tool_choice` stays `auto`; forced tool use returns a 400 on current models. Tools set `strict: true` with `additionalProperties: false`.
- **No sampling parameters:** no `temperature`, `top_p` or `top_k`; API version `2026-10-21` rejects them.
- **Refusals:** `stop_reason: "refusal"` is handled explicitly. Server-side fallback is on by default (`anthropic-beta: server-side-fallback-2026-07-01`, `fallbacks: "default"`), so a declined request is retried on a fallback model; a setting turns it off.
- **Prompt caching:** `cache_control` on the last tool and on the system prompt, which never contain timestamps or other changing text. When the table changes between questions, the new snapshot goes into the next user message; the cached prefix is never edited (§9).

**OpenAI-compatible** (`POST {base_url}/chat/completions`): `tools` with JSON-schema functions, `tool_calls` and `role: "tool"` results, SSE streaming, `usage` from the final chunk (`stream_options.include_usage`). Local servers often lack tool support; the provider dialog's **Test** button checks this and falls back to answer-only mode (§7.4) when tools aren't supported.

HTTP via `reqwest` with rustls, SSE parsing via `eventsource-stream`, both on tokio.

### 4.5 Tauri commands

| Command | Purpose |
|---|---|
| `ai_providers` / `ai_save_provider` / `ai_delete_provider` | Provider list (no keys) |
| `ai_set_key(provider_id, key)` / `ai_clear_key` / `ai_has_key` | Write-only key handling |
| `ai_test_provider(provider_id)` | Smallest request; reports reachable, tool support, model list |
| `ai_ask(tab_id, question)` → `run_id` | Start a run; events arrive on `ai://event` |
| `ai_cancel(run_id)` | Stop |
| `ai_apply_proposal(run_id, proposal_id)` | Applies through the existing `apply_op` / `run_sql` path and returns `TabSummary` |
| `ai_reject_proposal(run_id, proposal_id)` | Recorded in the chat history for the next turn |
| `ai_clear_chat(tab_id)` | Clear the conversation |

Each one gets a typed wrapper in `ui/src/lib/api.ts`.

---

## 5. Concurrency and the workspace lock

Today every command locks the window's `Workspace` for its whole duration. An AI run lasts seconds to minutes, so:

- **The LLM call never holds the lock.** The loop runs without it.
- **Read tools hold it only briefly.** Each one locks just long enough to read the tab's current query and table names, then unlocks and runs its SQL on a **separate DuckDB connection** (`Workspace::clone_connection`; views are shared because it is the same database). The user can page, sort and edit while the assistant works.
- **Proposals go through the normal path.** They are applied with the lock, exactly like a menu action, so undo and Applied Steps behave as usual.
- **Stale proposals are caught.** If the tab changed between the proposal and Apply (new steps, edits), the card shows "The table changed since this was proposed" and offers to re-run the preview.

---

## 6. Read-only SQL guard

`run_sql` and `propose_query` both pass this check (`parvu-core/src/sql_guard.rs`):

1. **One statement.** `extract_statements` must return exactly one.
2. **Only `SELECT` / `WITH … SELECT`.** `json_serialize_sql(?)` returns a parse tree only for SELECT statements and an error for anything else. That covers `INSERT`, `COPY`, `ATTACH`, `PRAGMA`, `SET`, `INSTALL`, `LOAD` and `CALL`.
3. **Only known tables.** Walk the tree: every `BASE_TABLE` reference must be an open tab name or a CTE defined in the same query, and **table functions are rejected** (`read_csv`, `read_parquet`, `read_json`, `glob`, `read_text`, `sniff_csv`, …). This stops the assistant from reading files the user never opened, for example `SELECT * FROM read_text('~/.ssh/id_ed25519')`, and sending them to the provider.
4. **Bounded.** Run on the AI connection, wrapped as `SELECT * FROM (<sql>) LIMIT 201`, with a 20-second deadline enforced by `InterruptHandle`.

Tests cover each rejected statement type, table functions hidden in subqueries and CTEs, quoted identifiers and comments. Expressions (`propose_expression`) already compile to SELECT-only SQL through `parvu-dsl`, and they pass the same check.

---

## 7. Approval and safety

### 7.1 Flow

```
model → propose_operation / propose_query / propose_expression
      → validate (serde / guard) → preview (DuckDB) → ProposalCard
user  → Apply  → ai_apply_proposal → Workspace::apply / run_sql → new Applied Step (undoable)
      → Reject → noted in chat history; the model sees it next turn
```

### 7.2 "Apply automatically this session"

- **Scope.** The checkbox applies to one proposal kind (for example filters) in one tab, for this session only. It is never saved.
- **Never auto-applied:** steps that remove columns or rows by default (drop columns, drop nulls, drop duplicates, `slice`), and any `propose_query`.
- **Visible.** The panel header shows what is currently auto-applied, with one click to revoke.

### 7.3 Failures

If an approved proposal fails in DuckDB, the error is shown on the card and offered back to the model as **Fix it**, one click, never automatic. The model's fix is another proposal. At most two fix attempts per question.

### 7.4 Answer-only mode

A setting, and the automatic fallback for models without tool support. The proposal tools are removed and the assistant describes the SQL instead. A **Run in editor** button puts that SQL into the query editor.

---

## 8. Credentials

### 8.1 Threat model
- **In scope:** other people or programs reading config files, and keys leaking through logs, crash reports, transcripts or screenshots of settings.
- **Out of scope:** malware running as the user, root, memory dumps. The docs say so.

### 8.2 Storage
- **Keychain.** Keys are stored with the `keyring` crate: Secret Service on Linux (GNOME Keyring, KWallet), macOS Keychain, Windows Credential Manager. The service is `ParVu`, the account is `ai/<key_ref>`.
- **Fallback.** When no keychain is available (some minimal Linux setups), a key can come from an environment variable named in the provider config (`env:ANTHROPIC_API_KEY`). We do not invent our own encrypted file: a passphrase scheme adds code and prompts for little gain on the machines where the keychain is missing. *(Open question §16.3.)*
- **Settings files hold names only.** `providers.json` stores `key_ref`, never a key.

### 8.3 Handling in memory
- Keys are read from the keychain just before a request, wrapped in `secrecy::SecretString`, and put in the HTTP header (`x-api-key` or `Authorization: Bearer`) marked sensitive.
- Keys never cross into the webview: `ai_set_key` is write-only, and no command returns a key.
- The provider dialog shows `●●●●` with a **Replace** button and has no way to reveal the key.

### 8.4 Redaction
- A `tracing` layer scrubs `sk-…`, `sk-ant-…`, `gsk_…`, `AIza…`, `Bearer …` and `x-api-key` values before anything is written.
- The crash report (`parvu-app`) runs the same scrub and never includes chat history.
- Text pasted into the chat that looks like an API key triggers a warning before sending.

---

## 9. Context

Every question sends:

1. **System prompt** (static, about 600 tokens, cached): ParVu's role, the rules (never invent columns, quote identifiers, prefer inspecting before answering, use proposals for changes, keep answers short, use Markdown tables), and the tool list.
2. **Table snapshot** (in the user message, not the system prompt, so the cached prefix stays stable):
   - the active tab's name, rows, and columns with type and null %, capped at about 2,000 tokens (wider tables say "N more columns; use describe_table");
   - the Applied Steps as their labels;
   - three rows from the start and three from the end, with long text cut and numbers rounded;
   - the names of other open tabs.
3. **History:** the last 8 exchanges of this tab's chat, with tool results shortened to their first rows.

Prompt caching makes the stable part cheap on Anthropic: tools, then system, then history, with breakpoints after the tools and after the system prompt. Reported `cache_read_input_tokens` appear in the activity log so a cache miss is visible.

No embeddings: SQL is the index. Sending rows to an embedding service is what local-first users want to avoid.

---

## 10. Tokens and cost

- **Usage.** Input, output and cache tokens are read from each response (`usage`). There is no client-side tokenizer.
- **Pricing.** A bundled price table (`parvu-ai/pricing.json`) covers the default models. A user override file (`~/.ParVu/ai/pricing.json`) handles other models; local models cost 0. For reference: Claude Opus 5.5 $4 / $20, Sonnet 5.5 $2 / $10, Haiku 5.5 $0.10 / $0.50 per million input / output tokens, cache reads cheaper.
- **Limits.** The activity strip shows the session total. A per-question limit (default 12 tool calls) and an optional session spending limit stop the loop when reached.

---

## 11. Settings and files

New `Settings` fields (serde defaults, so old settings files still load):

```rust
pub ai_enabled: bool,                 // false
pub ai_default_provider: Option<String>,
pub ai_answer_only: bool,             // false
pub ai_max_tool_calls: u32,           // 12
pub ai_history_turns: u32,            // 8
pub ai_session_cost_limit_usd: Option<f64>,
pub ai_server_fallback: bool,         // true (Anthropic refusals)
pub ai_panel_open: bool,
pub ai_panel_width: u32,              // 380
```

```
~/.ParVu/
├── settings/settings.json        + the fields above
└── ai/
    ├── providers.json            provider configs, no keys
    └── pricing.json              optional overrides
```

Chat history is kept in memory only (§16.4).

---

## 12. Logging and privacy

- **No secrets anywhere.** API keys never appear in logs, crash reports or transcripts. A test enforces this (§14).
- **Data in logs.** Prompts, row data and model answers are not logged at `info`. At `debug`, only sizes and timings are logged.
- **Transcript copy is sanitised by default.** Keys are removed and table rows replaced with `[n rows]`. A **Copy with data** option shows a warning first.
- **No telemetry.** ParVu sends nothing about AI use anywhere. The only network traffic is to the provider the user configured.
- **First-use notice.** The first time a cloud provider is used, a dialog explains what is sent: the schema, sample rows and query results.

---

## 13. Translations

All strings go through `t()`, with keys under `ai.*` (`ai.panel.title`, `ai.proposal.apply`, `ai.error.no_provider`, …) in `en`, `ru` and `az`. `cargo test -p parvu-config` checks that the files match. Operation step labels are reused for proposals, so their translations come free.

---

## 14. Testing

- **parvu-ai unit tests:**
  - wire-format encoding and decoding for both formats, using recorded SSE fixtures (text, tool use, parallel tool use, refusal, max_tokens, usage);
  - the agent loop against `fake.rs`: tool dispatch, all results in one message, a proposal ends the turn, cancellation, the tool-call cap;
  - pricing and redaction.
- **HTTP tests:** a local test server checks headers (`anthropic-version`, `x-api-key` / `Bearer`), streaming, timeouts and retries on 429/5xx with `retry-after`.
- **parvu-core:** the SQL guard (§6) and `preview_op` for every `Operation` variant, alongside `tests/transforms.rs`.
- **parvu-app:** `DataTools` over a real `Workspace` with test files.
- **UI (vitest):**
  - the panel renders a scripted conversation;
  - the proposal card's Apply calls `api.aiApplyProposal`;
  - Stop calls `api.aiCancel`;
  - the providers dialog never shows a key.
- **Privacy test:** a full fake run with a known fake key, then assert the key is absent from the log output, the crash report text and the sanitised transcript.
- **Real-provider smoke test:** opt-in through an environment variable, never in CI by default.

---

## 15. Milestones

**A: Data side (no AI yet). Done.**
- `parvu_core::readonly`:
  - `ReadSession`: a snapshot of every tab's current result on its own connection, with `query` (guarded, row cap, timeout), `describe_table`, `sample`, `value_counts`, `column_stats` and `check_operation`;
  - `guard_select`: the §6 guard.
- `Workspace::read_session`, `Workspace::preview_op` and `Workspace::build_op`.
- `parvu-core` feature `schema`: `schemars` derives and `ops::operation_schema()`.
- `DataTools` (§4.1) will be a thin trait in `parvu-ai` implemented over `ReadSession`.
- Plain table functions that read no files (`range`, `generate_series`, `unnest`) are allowed. `AddColumn` / `Replace` expressions are checked with the same guard, so a proposal can't read files either.

The original plan for A:
- A1. `preview_op` and the SQL guard in `parvu-core`, with tests.
- A2. Optional `schemars` derive on `Operation` and its parameter types, with doc comments on each variant.
- A3. The `DataTools` implementation in `parvu-app` over a cloned connection, with interrupt.

*Exit:* the tools work from a Rust test.

**B: `parvu-ai`.**
- B1. Neutral message types and the Anthropic provider (streaming, tools, caching, refusals).
- B2. The OpenAI-compatible provider.
- B3. The agent loop, events, fake provider, pricing, redaction.
- B4. Keychain storage and `providers.json` in `parvu-config`.
- B5. Tauri commands.

*Exit:* a test asks a question against the fake provider and gets tool calls and an answer, and a manual run works against Anthropic and Ollama.

**C: UI.**
- C1. Providers dialog (add, test, key handling).
- C2. AI panel, message list, tool-call rows, Markdown answers.
- C3. Proposal card with preview, Apply/Reject, session auto-apply.
- C4. Activity strip, Stop, the "Ask AI" editor mode, shortcuts.

*Exit:* ask, inspect, approve and undo end to end, in both themes and all three languages.

**D: Polish.**
- D1. Privacy test in CI, first-use notice, transcript copy.
- D2. `docs/AI.md` user guide; README and CHANGELOG.
- D3. Installer builds checked on Linux and Windows (keychain on both).

**E: MCP server** (§19.3).

**Later:** chat history saved per file (opt-in), "explain this table" one-click report, export a conversation as an SQL script, multi-tab join suggestions, one-click Ollama setup.

---

## 16. Open questions

1. **Default provider for new users.** Recommendation: none preselected. The providers dialog lists Anthropic first (`claude-opus-5-5`) and offers "Use a local model (Ollama)" as the privacy option.
2. **Server-side fallback on Anthropic refusals.** Recommendation: on by default (it retries a declined request on another model inside the same call), with a setting to turn it off.
3. **No keychain available.** An environment variable only (this plan), or also a passphrase-encrypted file?
4. **Chat history.** Recommendation: memory only for the first release; saving per file later, opt-in.
5. **Separate chat per tab, or one per window?** Recommendation: per tab, since questions are about one table.
6. **Build order.** Recommendation: MCP server first (milestones A then E1–E2). It gives AI access to ParVu data quickly, with no provider or key handling in ParVu, then the built-in assistant (B–D).

---

## 17. Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| The assistant reads files outside the open tabs | Medium | High | Table-function ban and table-name check in the SQL guard (§6), with tests |
| Runaway cost | Low | High | Tool-call cap, session spending limit, visible cost, Stop |
| A long query blocks the UI | Medium | Medium | Separate connection, 20 s interrupt, no workspace lock during LLM calls |
| Invented column names | High | Low | Schema in context; serde and DuckDB errors go back to the model; proposals preview before applying |
| Local models without tool calling | High | Low | Detected by Test; answer-only fallback |
| Provider API changes | Medium | Medium | Thin wire modules with recorded fixtures; pinned `anthropic-version` header |
| No keychain on a Linux setup | Low | Medium | Environment-variable fallback, documented |

---

## 18. New dependencies

| Crate | Where | Why |
|---|---|---|
| `reqwest` (rustls, stream, json) | parvu-ai | HTTP |
| `eventsource-stream`, `futures` | parvu-ai | SSE parsing |
| `tokio` (already in the tree through Tauri) | parvu-ai | async, timeouts |
| `secrecy` | parvu-ai, parvu-config | key wrapper |
| `keyring` | parvu-config | OS keychain |
| `schemars` (optional `schema` feature) | parvu-core | tool schemas from `Operation` |
| `rmcp` (server, stdio and Streamable HTTP transports) | parvu-app | MCP server (§19) |

Check the latest versions when milestone B starts. No new UI dependencies: Markdown uses `marked`, icons `@mdi/js`.

---

## 19. MCP interface

The Model Context Protocol (MCP) lets AI clients (Claude Desktop, Claude Code, IDE agents and others) call tools from local programs. ParVu already has the right tools for data work: the read-only tools of §3, the SQL guard of §6 and DuckDB underneath. MCP exposes them to agents the user already uses, with no API key in ParVu at all.

### 19.1 ParVu as an MCP server (first)

Two ways to run it:

**1. Headless: `parvu mcp` (stdio).** A command-line mode of the same binary, with no window. The client starts it; ParVu creates its own `Workspace` and serves tools over stdin/stdout. Client configuration:

```json
{ "mcpServers": { "parvu": { "command": "parvu", "args": ["mcp", "--allow-dir", "~/data"] } } }
```

**2. Attached to the running app (Streamable HTTP, later).** *Settings → AI → Allow AI clients to connect* starts a server on `127.0.0.1` with a random port and token, shown with a copyable client config. The agent then sees the tabs the user has open, and its proposals appear in ParVu's own approval cards.

Tools:

| Tool | Headless | Attached | Notes |
|---|---|---|---|
| `open_file(path)` | yes | no (the user opens files) | only under `--allow-dir` folders; formats as in `format.rs` |
| `list_tables` | yes | yes | |
| `describe_table`, `sample_rows`, `column_stats`, `value_counts` | yes | yes | same code as §3.1 |
| `query(sql)` | yes | yes | §6 guard; ≤ 1,000 rows (configurable) |
| `apply_operation(table, operation)` | yes, as a new step in the headless session | proposal card in the app | `Operation` JSON schema, as in §3.2 |
| `undo(table)` | yes | no | |
| `export(table, path, format)` | only with `--allow-write` | no | written only under allowed folders |

Resources: each open table as `parvu://table/<name>/schema`, so clients can attach a schema to a conversation without a tool call.

Safety:
- **File access** is limited to `--allow-dir` folders; the SQL guard still bans table functions, so queries can't reach other files.
- **Changes** in headless mode live only in that session's DuckDB views; nothing touches the user's files unless `--allow-write` is given.
- **Approval** of tool calls is the client's job (Claude Code and Claude Desktop ask by default). In attached mode ParVu adds its own approval cards on top.
- **Network:** the attached server listens on loopback only and requires the token; it is off by default and stops when the window closes.

Implementation: the official Rust SDK, `rmcp` (`modelcontextprotocol/rust-sdk`), in a new module of `parvu-app` (`mcp.rs`) on top of the same `DataTools` trait. The headless mode needs no webview, so `main.rs` checks for the `mcp` argument before starting Tauri.

### 19.2 ParVu's assistant as an MCP client (later)

Let the built-in assistant (§4) use tools from other MCP servers the user configures (a database, a documentation search…). It adds reach but also risk, because other servers' tools could exfiltrate table data. So it comes after the first release, with:
- per-server allow lists;
- every external tool call needing approval;
- a clear "this server can see your query results" notice.

### 19.3 Milestone E

- E1. `parvu mcp` stdio server with the read-only tools, `open_file` and `--allow-dir`; tested with the MCP Inspector and Claude Code.
- E2. `apply_operation`, `undo`, `export` behind `--allow-write`.
- E3. The attached Streamable HTTP mode with token and proposal cards (needs milestone C).
- E4. `docs/MCP.md`: client configs for Claude Desktop, Claude Code and VS Code.

E1–E2 depend only on milestone A, so the MCP server can ship before the built-in assistant. It is the cheapest way to get AI working with ParVu data, because users bring their own client and model.

---

## 20. Glossary

- **Proposal:** a change the assistant wants to make; it runs only when the user approves.
- **Read-only tool:** inspects data through the SQL guard; runs without asking.
- **Provider:** a configured LLM endpoint (kind, model, base URL, key name).
- **Answer-only mode:** proposal tools disabled; the assistant describes SQL instead.
