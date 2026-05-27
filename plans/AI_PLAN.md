# ParVu AI Milestone — Detailed Plan

> Status: design proposal, not yet implemented.
> Target version: `0.4.0` (AI-Preview) → `0.5.0` (AI-GA).
> Owner: @AzizNadirov.
> Last revised: 2026-05-27.

This document describes how AI is added to ParVu as a first-class user. It is written to be read top-to-bottom by an engineer who knows the codebase. Decisions already made are listed at the end of each section; truly open questions are collected in [§16 Open Questions](#16-open-questions).

---

## 1. Goals & Non-Goals

### Goals
- **Conversational data exploration.** User asks "what's the average price by category" in natural language; AI either answers from data it pulled via tools, or proposes a query for the user to approve and run.
- **One source of truth per capability.** Every operation (drop duplicates, replace, math op, join, append, …) lives in *one* place and is automatically: (a) an Operations-menu action, (b) a DSL expression function, (c) an AI tool. No three-way drift.
- **Local-first credentials.** API keys never leave the machine in plaintext, never appear in logs or crash reports, never travel to the bug-report endpoint.
- **Safe execution.** The AI cannot mutate state without an explicit user-visible confirmation step; "approve always for this session" is allowed.
- **Observable.** A collapsible terminal shows every tool call, prompt token count, completion tokens, latency, and cost estimate.
- **Provider-agnostic.** Anthropic, OpenAI, Gemini, OpenRouter, Groq, plus any OpenAI-compatible local endpoint (Ollama, LM Studio, llama.cpp server).

### Non-Goals (v0.4.0)
- Multi-turn autonomous agents that loop without the user. The agent runs to a single response or a single proposed action; the user decides what's next.
- Voice input.
- Fine-tuning / embeddings on the user's data (we use schema + samples, not vector DBs over row contents — see [§9 RAG](#9-rag--context-strategy)).
- Cloud sync of credentials or chat history.
- Plugin marketplace for tools (plugin system stays Python-side; AI sees registered tools via the capability registry).

### Decisions already made
- **Framework:** Pydantic AI.
- **Providers:** cloud + local OpenAI-compatible.
- **UI:** designed below in [§2](#2-uxui-design).

---

## 2. UX/UI Design

### 2.1 Layout

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│ MenuBar: File │ Edit │ Operations │ View │ AI ▾ │ Help                           │
├──────────────────────────────────────────────────────────────────────────────────┤
│ FileToolbar (existing)                                                           │
├───────────────────────────────────────────────────────────────┬──────────────────┤
│ QueryToolbar  ── [ SQL │ Expression │ Ask AI ]                │                  │
├───────────────────────────────────────────────────────────────┤                  │
│ QueryEditor (SQL / Expression / Ask)                          │   AI Chat Panel  │
│                                                               │   (dockable,     │
├───────────────────────────────────────────────────────────────┤    collapsible,  │
│ Applied Steps                                                 │    resizable)    │
├───────────────────────────────────────────────────────────────┤                  │
│                                                               │                  │
│ DataTableView (existing)                                      │                  │
│                                                               │                  │
│                                                               │                  │
├───────────────────────────────────────────────────────────────┴──────────────────┤
│ Pagination Bar (existing)                                                        │
├──────────────────────────────────────────────────────────────────────────────────┤
│ ▸ Terminal  ◾ 3 tool calls  ◾ 14.2k tokens  ◾ $0.0021  ◾ 1.2s   [⏹ Stop] [⎘]   │
└──────────────────────────────────────────────────────────────────────────────────┘
```

Three integration points:

1. **AI Chat Panel** — right-side `QDockWidget`, collapsible, default width 380 px, remembered per-session. Closed by default until first use.
2. **"Ask AI" mode in the editor toolbar** — third radio next to SQL / Expression. User types a question in the editor and presses Execute → spawns AI run, panel opens automatically, response streams in.
3. **Collapsed terminal** — bottom strip, always visible, click `▸` to expand. Shows live tool-call log, token counter, cost, cancel button. When collapsed it is a 28-px-high status strip with the live counters.

### 2.2 Chat panel anatomy

```
┌──────────────────────────────────────┐
│  AI · claude-sonnet-4-6 ▾   ⚙   ✕   │  ← provider/model picker + settings
├──────────────────────────────────────┤
│ ┌──[ user ]─────────────────────┐    │
│ │ how many nulls in price col?  │    │
│ └───────────────────────────────┘    │
│                                      │
│ ┌─[ assistant ]──────────────────┐   │
│ │ Checking…                      │   │
│ │ ▸ called column_null_count     │   │
│ │ There are 1,204 nulls in price │   │
│ │ (3.1% of 38,712 rows).         │   │
│ │                                │   │
│ │ Want to drop them? [Suggest]   │   │
│ └────────────────────────────────┘   │
│                                      │
│ ┌─[ assistant — proposed action ]┐   │
│ │ DELETE rows WHERE price IS NULL│   │
│ │ Estimated impact: -1,204 rows  │   │
│ │ [ ✓ Approve ] [ ✗ Reject ]     │   │
│ │ ☐ Always approve this session  │   │
│ └────────────────────────────────┘   │
├──────────────────────────────────────┤
│ [ ➤ Ask anything… ]              ⏎  │
└──────────────────────────────────────┘
```

- Messages are `QFrame` cards with role-coloured left border.
- Tool calls render as a one-line collapsible row (`▸ called <tool>(args)`) inside the assistant card. Clicking expands to show full args + result preview.
- Proposed actions render as a distinct card type with an Approve/Reject affordance — never as plain text.

### 2.3 Terminal anatomy

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│ ▾ Terminal                                                  Clear  Copy  Export ▾│
├──────────────────────────────────────────────────────────────────────────────────┤
│ 14:32:08.123 [agent]   user: "how many nulls in price col?"                      │
│ 14:32:08.412 [llm]     → request prompt=312 tok, model=claude-sonnet-4-6         │
│ 14:32:09.001 [tool]    column_null_count(column="price")                         │
│ 14:32:09.118 [tool]    → 1204 (took 117ms)                                       │
│ 14:32:09.612 [llm]     ← response prompt=312 completion=84 tot=396 cost=$0.0021  │
│ 14:32:09.612 [agent]   done in 1.49s                                             │
└──────────────────────────────────────────────────────────────────────────────────┘
```

- Monospace, scroll-locked-to-bottom unless user scrolls up (then a "↓ jump to live" pill appears).
- Lines are tagged `[agent]` / `[llm]` / `[tool]` / `[error]` and colour-coded by theme.
- "Export" produces a sanitised JSONL of the session — API keys, raw prompts, raw responses are redacted (see [§12 Logging & Privacy](#12-logging--privacy)).

### 2.4 Theming & i18n

- All new widgets implement `Themeable` (see `src/parvu/presentation/themeable.py`) — colours come from the theme manager.
- All new strings live in `i18n/locales/{en,ru,az}.json`. No hard-coded English in widget code.
- New theme tokens: `ai_chat_user_bg`, `ai_chat_assistant_bg`, `ai_tool_call_fg`, `ai_proposed_action_border`, `terminal_tag_agent`, `terminal_tag_tool`, `terminal_tag_llm`, `terminal_tag_error`. Defaults defined for the three built-in themes.

### 2.5 Keyboard

| Action | Shortcut |
|---|---|
| Toggle AI panel | `Ctrl+Shift+A` |
| Focus AI input | `Ctrl+L` |
| Toggle terminal | `Ctrl+\`` |
| Stop running agent | `Esc` while panel focused |
| Approve proposed action | `Ctrl+Enter` on the action card |

---

## 3. The Capability Layer — One Definition → UI + Expression + Tool

This is the structural fix that prevents spaghetti. Today, "drop duplicates" exists three times:
- `presentation/dialogs/drop_duplicates_dialog.py` (UI)
- `core/dsl/registry.py::DROP_DUPLICATES` (expression)
- *(implicit)* the SQL it generates lives in the dialog's accept handler.

We introduce a **Capability**: a single declarative object that drives all three surfaces.

### 3.1 Capability schema

```python
# src/parvu/core/capabilities/base.py

class Capability(BaseModel):
    """A user-facing data operation, exposed in 3 surfaces:
       UI dialog, DSL expression, AI tool."""

    id: str                          # "drop_duplicates"
    name_key: str                    # i18n key — "capability.drop_duplicates.name"
    description_key: str             # i18n key — "capability.drop_duplicates.desc"
    category: Literal["transform", "filter", "aggregate", "io", "inspect"]

    params: list[ParamSpec]          # typed, validated, rich

    # Pure compiler: (params, context) -> DuckDB SQL string OR ir.Step
    compile: Callable[[BoundParams, CompileContext], CompiledStep]

    # Optional: previewer for the UI dialog
    preview_rows: Callable[[BoundParams, CompileContext], pd.DataFrame] | None = None

    # Surface flags
    show_in_operations_menu: bool = True
    show_in_expression: bool = True
    show_in_ai_tools: bool = True

    # AI-only metadata
    ai_safety: Literal["read_only", "mutates_view", "destructive"] = "mutates_view"
    ai_examples: list[str] = []      # few-shot examples for the agent
```

```python
class ParamSpec(BaseModel):
    name: str
    kind: Literal["column", "columns", "table", "string", "int", "float",
                  "bool", "enum", "regex", "expression"]
    required: bool = True
    default: Any = None
    label_key: str
    help_key: str | None = None
    enum_values: list[str] | None = None
    column_filter: ColumnTypeFilter | None = None   # "numeric only" etc.
    ui_widget_hint: str | None = None               # "combo", "spinner", "checkbox"
```

### 3.2 How surfaces consume a Capability

```
                       ┌────────────────────────┐
                       │  CapabilityRegistry    │
                       │  (singleton in DI      │
                       │   container)           │
                       └────────────┬───────────┘
                                    │
        ┌───────────────────────────┼──────────────────────────────┐
        ▼                           ▼                              ▼
┌────────────────┐         ┌────────────────────┐         ┌────────────────┐
│ Operations     │         │  DSL Registry      │         │  AI Tool       │
│ menu builder   │         │  (existing)        │         │  factory       │
│                │         │                    │         │                │
│ For each cap   │         │ For each cap with  │         │ For each cap   │
│ where          │         │ show_in_expression │         │ where          │
│ show_in_ops:   │         │ : wraps compile()  │         │ show_in_ai_    │
│ build a dialog │         │ as a registry      │         │ tools: emits a │
│ from ParamSpec │         │ FunctionDef.       │         │ Pydantic-AI    │
│ list, validate │         │                    │         │ tool with      │
│ inputs, call   │         │                    │         │ generated      │
│ capability.    │         │                    │         │ pydantic args  │
│ compile().     │         │                    │         │ model + same   │
│                │         │                    │         │ compile path.  │
└────────────────┘         └────────────────────┘         └────────────────┘
```

- The Operations dialogs become **one** generic `CapabilityDialog` that introspects `ParamSpec`s. Specialised dialogs only remain where the UI demands something the generic widget can't render (e.g. unique-values filter with search). Even then, they delegate the *compile* step back to the capability.
- The DSL registry stops hand-registering `DROP_DUPLICATES`, etc. — it iterates `CapabilityRegistry.all()` at startup and registers each one whose `show_in_expression` is true.
- AI tools are generated the same way at startup: each capability becomes a Pydantic-AI tool whose argument model is built from `ParamSpec`s.

### 3.3 Migration path (won't be one big bang)

1. Land `Capability` + `CapabilityRegistry` empty.
2. Move existing ops one at a time: `drop_duplicates` → `replace_values` → `math_op` → `join` → `append` → `sort_by_column` → `head`/`tail`/`sample` (new). Each migration is one PR, deletes ~30–80 lines of duplication.
3. Once the last op is migrated, delete `core/dsl/registry.py::_register_defaults()`'s table-op block and let it source from the capability registry.
4. AI tool generation is wired in *after* all ops are migrated, so we never expose a half-finished tool to the LLM.

### 3.4 New capabilities introduced specifically for AI

These are pure data-inspection tools the agent will lean on. They're cheap, read-only, and exposed in the menu under **View → Quick Inspect** so a power user can use them too.

| Capability id | What it does | AI safety |
|---|---|---|
| `head` | First N rows (default 5) | read_only |
| `tail` | Last N rows | read_only |
| `sample_random` | Random N rows (seeded) | read_only |
| `describe_table` | Column names, types, null counts, distinct counts, min/max for numerics | read_only |
| `column_value_counts` | Top-K distinct values + counts for a column | read_only |
| `column_null_count` | Null count + percentage for a column | read_only |
| `row_count` | Total rows for current query | read_only |
| `schema_summary` | Compact column → type mapping for all open tabs | read_only |
| `run_sql_readonly` | Execute a `SELECT`-only SQL string, return ≤200 rows | read_only (validated) |

`run_sql_readonly` is gated by a SQL AST check (sqlglot) — reject anything that isn't a pure `SELECT`/`WITH`. Anything mutating goes through the proposed-action flow.

---

## 4. AI Infrastructure

### 4.1 Stack

```
pydantic-ai            agent loop, tool dispatch, structured outputs
└── pydantic-ai-slim   optional, smaller if we don't need extras
pydantic               (already in deps)
httpx                  (pulled in by pydantic-ai)
cryptography           credential encryption
keyring                OS-native secret storage (fallback)
tiktoken               token counting for OpenAI/local; for Anthropic use their /v1/messages/count_tokens
```

### 4.2 Module layout (new code)

```
src/parvu/ai/
├── __init__.py
├── agent.py                  # ParVuAgent: wraps pydantic_ai.Agent, system prompt, lifecycle
├── providers/
│   ├── __init__.py
│   ├── base.py               # ProviderConfig, ProviderRegistry
│   ├── anthropic.py
│   ├── openai.py
│   ├── gemini.py
│   ├── openrouter.py
│   ├── groq.py
│   └── openai_compatible.py  # Ollama / LM Studio / llama.cpp / any custom base_url
├── credentials/
│   ├── __init__.py
│   ├── store.py              # CredentialStore: encrypted blob + keyring fallback
│   ├── crypto.py              # Fernet wrapper, key derivation
│   └── redaction.py          # log/crash redaction filters
├── tools/
│   ├── __init__.py
│   ├── factory.py            # capability → pydantic-ai tool
│   └── safety.py             # SafetyClassifier, ProposedAction
├── context/
│   ├── __init__.py
│   ├── builder.py            # build system prompt + context snapshot
│   └── snapshot.py            # cheap-to-compute table snapshot for context window
├── terminal/
│   ├── __init__.py
│   ├── event_bus.py          # AgentEvent stream
│   └── recorder.py           # in-memory ring buffer + sanitised export
├── tokens/
│   ├── __init__.py
│   ├── counter.py            # ITokenCounter; tiktoken / Anthropic counter / heuristic
│   └── pricing.py            # static pricing table → cost estimate

src/parvu/presentation/widgets/
├── ai_chat_panel.py
├── ai_terminal.py
└── ai_message_card.py

src/parvu/presentation/dialogs/
├── ai_credentials_dialog.py
├── ai_provider_dialog.py
└── ai_proposed_action_dialog.py    # if/when a separate window is needed
```

### 4.3 Agent shape

```python
# src/parvu/ai/agent.py

class ParVuAgent:
    def __init__(self, container: ServiceContainer, provider: ProviderConfig):
        self._container = container
        self._provider = provider
        self._tools = build_tools_from_capabilities(container.capability_registry)
        self._agent = pydantic_ai.Agent(
            model=provider.to_pydantic_ai_model(),
            tools=self._tools,
            system_prompt=build_system_prompt(container),
            instrument=False,   # we ship our own event bus
        )
        self._bus = AgentEventBus()

    async def ask(self, user_message: str, *, on_event: Callable[[AgentEvent], None]) -> AgentResponse:
        self._bus.subscribe(on_event)
        try:
            result = await self._agent.run(user_message, deps=self._build_deps())
            return AgentResponse(text=result.output, usage=result.usage())
        finally:
            self._bus.unsubscribe(on_event)
```

- **No streaming in v0.4.0.** It complicates Qt threading. Add in 0.5.0.
- The agent loop is asyncio; we run it on a `QThread` with its own event loop via `asyncio.new_event_loop()`. Events come back to the GUI thread through `pyqtSignal`s.

### 4.4 Provider configs

```python
class ProviderConfig(BaseModel):
    id: str                          # unique within store: "anthropic-default", "ollama-laptop"
    kind: Literal["anthropic", "openai", "gemini", "openrouter", "groq", "openai_compatible"]
    display_name: str
    model: str                       # "claude-sonnet-4-6", "llama3.1:8b"
    base_url: str | None = None      # only for openai_compatible
    api_key_ref: str | None = None   # logical name in CredentialStore, NEVER the key itself
    extra_headers: dict[str, str] = {}
    max_tokens: int = 4096
    temperature: float = 0.2
    request_timeout_s: int = 60
```

`api_key_ref` is a logical name like `"anthropic-default"`. The actual key is read from the credential store at request time and never stored on this object.

### 4.5 System prompt strategy

Built fresh per request from a template + live context. Sketch:

```
You are an analyst assistant inside ParVu, a desktop viewer for Parquet/CSV/JSON.

Available data:
- Active table: "{table_name}" — {row_count} rows, {col_count} cols
- Columns: {compact_schema}
- Other open tabs: {other_tabs_summary}

You have tools to inspect and transform tables. Prefer inspecting (head, sample,
describe_table, column_value_counts, run_sql_readonly) before answering. If the
user asks for a transformation, do NOT run it yourself — propose it via the
`propose_action` tool and let the user approve.

Hard rules:
- Never invent column names. If unsure, call describe_table or schema_summary first.
- Quote SQL identifiers when they contain spaces or are reserved words.
- Answers must be concise. Tables in markdown. No filler.
```

The compact schema is capped at ~2k tokens; if the table has hundreds of columns, we hash-prioritise (numeric + recently-clicked columns first) and tell the model "schema truncated, use schema_summary to fetch more."

---

## 5. Credentials — Local, Encrypted, Never Logged

### 5.1 Threat model

In scope:
- Casual local attackers reading config files.
- Accidental leakage via logs, crash reports, terminal export, screen-share.

Out of scope:
- Root-level attackers, malware running as the user, physical memory dumps. (We document this; we don't pretend otherwise.)

### 5.2 Storage

Two-layer, with graceful fallback:

1. **Preferred: OS keyring** via the `keyring` package — Windows DPAPI (Credential Manager), macOS Keychain, freedesktop Secret Service. The actual API key is stored here keyed by `parvu/{api_key_ref}`.
2. **Fallback: Fernet-encrypted blob** at `~/.ParVu/secrets/credentials.enc`. Used when keyring isn't available (some Linux headless setups). Key derivation:
   - On first run: generate a random 32-byte master key, store **only in keyring** under `parvu/master`. So the blob is encrypted and the key is still OS-protected.
   - If keyring is *also* unavailable: prompt the user for an unlock passphrase on each launch, derive via Argon2id. Document this clearly: weak passphrase = weak protection.

`credentials.enc` schema:

```json
{
  "version": 1,
  "entries": [
    {"ref": "anthropic-default", "ciphertext": "...", "nonce": "..."},
    {"ref": "openai-personal",   "ciphertext": "...", "nonce": "..."}
  ]
}
```

File mode `0600` on Unix; `icacls` to restrict to current user on Windows.

### 5.3 Loading & lifetime

- Keys are read **just-in-time** before the HTTP request and held in a local variable. They are never stored on `ProviderConfig`, never in `Settings`, never passed across thread boundaries as plain attributes — they live inside the closure that builds the HTTP client.
- A `SecretString` wrapper (`pydantic.SecretStr`-compatible) is used wherever the key must travel. Its `__repr__` / `__str__` always return `"***"`.

### 5.4 Redaction

- Loguru sink filter that replaces anything looking like `sk-...`, `sk-ant-...`, `gsk_...`, or whitespace-following `Authorization:` with `***`.
- Crash reporter (`src/parvu/crash_reporter.py`) gets the same filter applied to the report body before any user-visible preview.
- The Terminal export uses a separate, stricter filter that also strips full prompt/response bodies (configurable).

### 5.5 Credentials UI

`AICredentialsDialog`:

```
┌───────────────────── AI Providers ─────────────────────┐
│  ┌────────────────────┬─────────────────────────────┐ │
│  │ Anthropic (default)│ Provider:  Anthropic ▾      │ │
│  │ OpenAI             │ Display:   Anthropic default│ │
│  │ Ollama laptop      │ Model:     claude-sonnet… ▾ │ │
│  │ + Add provider     │ Base URL:  (n/a)            │ │
│  │                    │ API Key:   ●●●●●●●●●●●●●●●  │ │
│  │                    │            [ Replace ]      │ │
│  │                    │ Test:      [ ✓ Reachable ]  │ │
│  │                    │                             │ │
│  │                    │ [ Delete ]  [ Set Default ] │ │
│  └────────────────────┴─────────────────────────────┘ │
│                                                        │
│  [ Cancel ]                              [ Save ]      │
└────────────────────────────────────────────────────────┘
```

- Add/remove/edit providers.
- "Test" performs the smallest possible request (e.g. `models.list` or a 1-token completion). Result shown inline; key never logged.
- The key field is *write-only*: once saved, the dialog shows `●●●●` and a "Replace" button. There is no path to display the cleartext key in the UI — keep the principle simple.

---

## 6. Tool Catalog

(Already partly covered in [§3.4](#34-new-capabilities-introduced-specifically-for-ai). This section lists the *full* set the agent sees in v0.4.0.)

### 6.1 Read-only inspection tools

| Tool | Args | Returns | Notes |
|---|---|---|---|
| `head` | `n: int = 5, table: str = active` | rows as records | capped at 50 |
| `tail` | `n: int = 5` | rows | capped at 50 |
| `sample_random` | `n: int = 10, seed: int = 42` | rows | capped at 50 |
| `describe_table` | `table: str = active` | per-column stats | short-circuits if cached |
| `schema_summary` | `tables: list[str] = all_open` | `{table: [{name, type, nulls}]}` | always cheap |
| `column_value_counts` | `column: str, k: int = 10` | `[(value, count), ...]` | capped at k=50 |
| `column_null_count` | `column: str` | `{nulls, total, pct}` | |
| `row_count` | `table: str = active` | int | |
| `run_sql_readonly` | `sql: str` | up to 200 rows + truncation flag | sqlglot AST validated |
| `list_open_tabs` | — | list of tab names | |

### 6.2 Action-proposing tools

The agent **cannot mutate state directly**. To request a mutation it calls:

```python
@tool
def propose_action(
    capability_id: str,
    params: dict,
    rationale: str,
) -> ProposedActionAck:
    """Propose a transform/filter/aggregate. Returns an ack id; the user
    will approve or reject in the UI. You will NOT see the result of the
    action in this turn."""
```

The tool resolves the capability, validates params against its `ParamSpec`, compiles a dry-run preview (using `Capability.preview_rows`), and emits a `ProposedAction` event to the UI. The agent's turn ends after `propose_action` is called — there's no continuation in the same run. (Multi-step: the user approves, the UI runs it, and if they want they ask the AI a follow-up; the agent sees the new state on the next turn.)

This is deliberate — it prevents the model from chaining ten mutating ops in one turn before the user can react.

### 6.3 Capability-derived action tools (read-only execution path)

For pure-inspection capabilities (`head`, `tail`, etc.), the agent can call them *directly*; no approval needed. Safety flag on the Capability controls this.

---

## 7. Safety of Execution — Human-in-the-Loop

### 7.1 Approval flow

```
agent → propose_action(capability_id, params, rationale)
      ↳ ToolFactory validates params, runs preview_rows() if available
      ↳ emits ProposedAction event → UI renders an action card
      ↳ user clicks Approve / Reject
          ├─ Approve → CapabilityRunner.run() → applied as a normal Applied Step
          │           ↳ on failure: error → AI follow-up loop (§7.3)
          └─ Reject → emits Rejection event; AI sees it on next user turn
```

### 7.2 "Always approve" scope

- The "Always approve this session" checkbox is **per (capability, session)** — not per-capability-forever. We do not want the AI to develop muscle memory of the user being permissive.
- We expose a session-policy view in the AI panel header showing what's been auto-approved. One click to revoke.
- Destructive capabilities (`ai_safety="destructive"`) **cannot** be added to always-approve. They always prompt.

### 7.3 Failure feedback loop

If the user approves and the SQL fails (DuckDB error), we:

1. Render the error in the action card.
2. Build a follow-up agent turn automatically with the error text + the SQL that failed + the schema snapshot. (User sees the request being made and a one-click "Cancel auto-retry".)
3. The agent tries again — but **always** ends with another `propose_action` if it wants to mutate. No exception.
4. Max 2 auto-retries per original user message. Beyond that, hand back to the user.

### 7.4 Read-only escape valve

If the user *only* wants Q&A and never wants approval prompts, a setting **"AI mode: Answer only (never propose actions)"** disables `propose_action` entirely. The agent will then describe the SQL it would run, instead of asking to run it.

---

## 8. Terminal & Token Counter

### 8.1 Event bus

```python
class AgentEvent(BaseModel):
    ts: datetime
    kind: Literal["agent.start", "agent.end",
                  "llm.request", "llm.response",
                  "tool.call", "tool.result", "tool.error",
                  "action.proposed", "action.approved", "action.rejected",
                  "agent.error"]
    payload: dict
```

The agent emits events; the terminal widget and the chat panel both subscribe. Events are also written to an in-memory ring buffer (last 5000 events) so "Export session" works even after a long conversation.

### 8.2 Token counter

A live status strip aggregates:
- **Prompt tokens** — counted before request via `ITokenCounter` (tiktoken for OpenAI/local OpenAI-compatible; Anthropic's `count_tokens` for Anthropic; heuristic ≈ `len(text) // 4` for Gemini/Groq if SDK doesn't expose it).
- **Completion tokens** — read from provider response.
- **Total cost** — looked up in `pricing.py`. Editable JSON so the user can override or fill in for local models (default: `0`).

```python
class TokenMeter(BaseModel):
    requests: int = 0
    prompt_tokens: int = 0
    completion_tokens: int = 0
    estimated_cost_usd: float = 0.0

    def add(self, usage: Usage, provider: ProviderConfig) -> None: ...
```

The status strip shows the *session* meter; click for a per-turn breakdown.

### 8.3 Stop button

`Esc` while the panel is focused, or the `⏹ Stop` button, cancels the active agent run. Implementation: the QThread's loop calls `task.cancel()` and waits up to 2s; the UI shows "Cancelled" and the terminal logs `[agent] cancelled by user`. Any in-flight tool call is awaited to completion (we cannot interrupt DuckDB safely mid-query without risking corruption) — but no further LLM calls or tools are issued.

---

## 9. RAG / Context Strategy

Full-table embeddings are overkill and leak data. We do **schema-aware context priming**, not RAG over rows.

### 9.1 What's in the context window every turn

1. **System prompt** (~600 tokens, static).
2. **Compact schema** of the active tab — column names, types, null %. Capped at ~2k tokens; truncated with `schema_summary` tool fallback.
3. **Small sample** — 3 rows from `head`, 3 from `tail`. Capped at ~500 tokens; numeric columns rounded.
4. **Last N message turns** — N=8 default, configurable.
5. **Applied Steps summary** — short description of transforms the user has run this session ("dropped 1,204 null-price rows", "sorted by category asc"). Capped at ~300 tokens.

Total static budget ≈ 3.5–4k tokens, leaving room for tool results.

### 9.2 When the schema doesn't fit

For wide tables (>200 cols), the priming context becomes "schema summary: 320 columns of mixed types — use `schema_summary(filter=...)` to fetch what you need." The agent then iterates with the tool until it has what it needs.

### 9.3 Why not row embeddings?

- Privacy. Sending row contents to an embedding API is exactly what the user is *trying to avoid* by running ParVu locally.
- Cost & complexity. We'd need a vector store, an embedding model, and an update path on every transform.
- The actual question is almost always a SQL aggregate. SQL is the index; let the agent write SQL.

(If we ever do need full-text search over many tables, a duckdb FTS index over `read_csv_auto(... LIMIT 1000)` previews is a much smaller hammer.)

---

## 10. Threading & Cancellation

PyQt6 + asyncio is a known footgun. Plan:

- The agent runs on a `QThread` (`AIWorkerThread`) with `asyncio.new_event_loop().run_until_complete(...)`.
- The thread communicates via three `pyqtSignal`s:
  - `event_emitted(AgentEvent)`
  - `partial_text(str)` (placeholder for v0.5 streaming)
  - `finished(AgentResponse)`
- DuckDB tool calls go through the existing `QueryEngine` — which is **not** thread-safe across the same connection. We open a **separate read-only DuckDB connection** for the AI agent's tools. Cell edits and Applied Steps continue to run on the main connection. When the user approves a proposed action, it's applied via the main connection on the GUI thread (or a worker, same as today's transforms).

Cancellation:
- Stop button calls `worker.cancel()` → cancels the asyncio task.
- DuckDB queries are not cancellable mid-execution in our version; we accept that the current tool call finishes, then the agent stops. For `run_sql_readonly` we additionally apply a server-side `SET statement_timeout` so a runaway agent query has an upper bound.

---

## 11. Settings & Config

New `Settings` fields (`config/settings.py`):

```python
# AI
ai_enabled: bool = False                       # off by default until user configures
ai_default_provider_id: str | None = None
ai_max_context_turns: int = 8
ai_max_auto_retries: int = 2
ai_answer_only_mode: bool = False
ai_terminal_expanded: bool = False
ai_panel_visible: bool = False
ai_panel_width: int = 380
ai_show_cost_estimates: bool = True
ai_pricing_overrides_path: str | None = None   # optional JSON path
```

Provider configs and approved-capability lists are **not** in `Settings` — they live in their own files under `~/.ParVu/ai/` so they're easy to inspect/back up separately.

```
~/.ParVu/
├── settings/settings.json
├── secrets/credentials.enc        # encrypted blob (mode 0600)
├── ai/
│   ├── providers.json             # provider configs, no keys
│   ├── pricing_overrides.json     # optional
│   └── chat_history/{session_id}.jsonl    # sanitised, opt-in
```

---

## 12. Logging & Privacy

### 12.1 Non-negotiables

- **API keys never logged.** Enforced by a loguru `patcher` that scrubs records before serialisation. Tests assert this.
- **Crash reports never include keys, prompts, responses, or row data.** The crash reporter gets a separate pre-flight pass that strips all of `~/.ParVu/ai/chat_history/`, `secrets/`, and any in-memory event ring buffers.
- **Terminal export** is opt-in and runs through a sanitiser by default. Three export modes:
  - **Sanitised** — keys & raw prompts/responses redacted. Default.
  - **With prompts/responses** — explicit opt-in, shown a warning dialog. Useful for bug reports about the AI itself.
  - **Raw** — disabled in release builds. Devs flip a build flag.

### 12.2 Telemetry

- ParVu does not phone home with AI usage. Token meters and costs are computed locally.
- Provider SDKs may emit their own telemetry; we set `disable_telemetry=True` where supported (e.g. some SDKs send anonymous metrics by default).

---

## 13. Internationalisation

All new strings go through the existing i18n system. New translation keys are namespaced under `ai.*` and `capability.*`:

```
ai.panel.title
ai.panel.placeholder
ai.panel.send
ai.action.approve
ai.action.reject
ai.action.always_approve
ai.terminal.title
ai.terminal.export
ai.credentials.title
ai.credentials.test_button
ai.error.no_provider_configured
...
capability.head.name
capability.head.desc
capability.head.param.n.label
...
```

Russian and Azerbaijani translations land in the same PR that introduces each string (existing project rule).

---

## 14. Testing

### 14.1 Unit
- `CredentialStore` round-trip; keyring-unavailable fallback; mode bits; redaction filter.
- Each capability: param validation, `compile()` correctness against a tiny in-memory duckdb table, preview rows.
- `run_sql_readonly` SQL AST validator: reject INSERT/UPDATE/DELETE/CREATE/DROP/PRAGMA/CALL.
- Token counter: deterministic counts for known strings per provider.

### 14.2 Integration
- Fake-provider mode (`pydantic_ai.models.test.TestModel`) runs the agent end-to-end without an API key. Used in CI for: tool-dispatch correctness, propose-action flow, retry-on-error flow, cancellation.
- Real-provider smoke tests are **off by default**, opt-in via env var, run nightly with a tiny budget.

### 14.3 GUI
- pytest-qt: chat panel renders, approve/reject buttons emit signals, terminal expands/collapses, credentials dialog saves and reads back.
- Visual regression (optional): screenshots of chat / terminal in all three themes.

### 14.4 Privacy assertions
- A pytest test that triggers a full agent turn, then asserts no log line, no crash-report payload, and no terminal-export-sanitised buffer contains the test API key string. Runs in CI.

---

## 15. Roadmap & Milestones

### Milestone A — Capability Refactor (no user-visible AI yet)  — **2 weeks**
- A1. `Capability`, `ParamSpec`, `CapabilityRegistry`, `CompileContext` skeleton.
- A2. Generic `CapabilityDialog` widget. New `View → Quick Inspect` menu.
- A3. Migrate `drop_duplicates`, `replace_values`, `math_op`, `join`, `append`, `sort_by_column` to capabilities.
- A4. Migrate DSL registry to source table-ops from capabilities.
- A5. Add inspection capabilities: `head`, `tail`, `sample_random`, `describe_table`, `column_value_counts`, `column_null_count`, `row_count`, `schema_summary`, `run_sql_readonly`.
- A6. Tests for all migrated capabilities.

Exit criteria: app behaves identically to today; codebase shorter; one path for new ops.

### Milestone B — AI Infrastructure  — **2 weeks**
- B1. `parvu.ai.providers` + `parvu.ai.credentials` with full test coverage.
- B2. `AICredentialsDialog`.
- B3. Settings additions.
- B4. `parvu.ai.tokens` + pricing table.
- B5. `parvu.ai.agent.ParVuAgent` against `TestModel`.
- B6. `AIWorkerThread` and signal plumbing.

Exit criteria: a CLI smoke script can: configure a provider, send a question, see tool calls and final answer in the terminal, all without GUI.

### Milestone C — AI UI  — **2 weeks**
- C1. `AIChatPanel` widget, dockable, themeable, i18n'd.
- C2. `AITerminal` widget, collapsible status strip + expanded view.
- C3. Message cards (user / assistant / tool-call / proposed-action).
- C4. Approve/Reject flow + "always approve session" scope.
- C5. Stop button.
- C6. "Ask AI" mode in QueryToolbar.

Exit criteria: end-to-end demo against Anthropic + Ollama; user can ask, inspect, approve, undo.

### Milestone D — Polish, Privacy, Docs  — **1 week**
- D1. Logging/redaction audit + privacy test in CI.
- D2. Terminal export modes.
- D3. Failure-feedback loop with retry cap.
- D4. `docs/AI.md` user-facing guide.
- D5. Release notes; bump to `0.4.0`.

Exit criteria: privacy tests green, docs published, RC build tested on Windows + Linux.

### Post-0.4 (0.5.x and beyond)
- Streaming responses.
- Multi-tab join suggestions (agent proposes joins across open tabs).
- Recipe export — turn a chat sequence into a reproducible Python/SQL script.
- Read-only "explain this data" report (one-click, generates a markdown summary).
- Optional: embeddings index for column-name search across many files.

---

## 16. Open Questions

1. **Where does the AI panel dock by default — right or bottom?** Recommendation: right. Bottom is heavily used by the table view + applied steps + terminal already.
2. **Pricing data freshness.** Static JSON in-repo vs. fetched once a week. Recommendation: static; user can override via `ai_pricing_overrides_path`. Pricing data is small and the user has bigger fish to fry than fresh decimal places.
3. **Should chat history persist by default?** Recommendation: no. Off by default; enable per-session via a checkbox. When on, stored sanitised under `~/.ParVu/ai/chat_history/`.
4. **Should we ship a one-click "Run a local model" path?** I.e. detect Ollama, list installed models, autoconfigure. Probably yes in 0.4.1 — it's a strong privacy story and a great onboarding moment.
5. **Multi-tab awareness on day one?** Recommendation: yes — `schema_summary` already accepts a tabs list. The agent should know about other open tabs but should only operate on the active one unless the user names another.

---

## 17. Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Pydantic-AI API churn | Medium | Medium | Pin version; thin wrapper in `ai/agent.py`; integration test against `TestModel` catches breakage on upgrade. |
| Keyring unavailable on user's distro | Low | Medium | Fernet-blob fallback with passphrase prompt; documented. |
| Agent racks up cost with a runaway loop | Low | High | Hard cap on tool calls per turn (default 12); statement timeout on `run_sql_readonly`; visible cost meter; Stop button bound to Esc. |
| User pastes a key into the chat input by accident | Medium | High | Outbound redaction filter on the chat input — if a token looks like an API key, prompt before sending and redact it from the prompt. |
| LLM hallucinates a column name | High | Low | System prompt forbids it; `describe_table` is cheap; failed-SQL feedback loop self-corrects. |
| DuckDB query inside tool blocks for minutes | Medium | Medium | Statement timeout on read-only tools; Stop button cancels the agent loop (tool call still finishes). |
| Wide tables exceed schema context budget | Medium | Low | Truncate + fallback to `schema_summary` tool. |

---

## 18. Dependency Diff

Add to `pyproject.toml`:

```toml
"pydantic-ai>=0.0.50",       # pin minor; verify latest at start of milestone B
"keyring>=24.0",
"cryptography>=42.0",
"tiktoken>=0.7",             # only needed for OpenAI/local counting; lazy-imported
```

Optional dev:
```toml
"respx>=0.21",                # mock httpx for provider unit tests
```

No removals.

---

## 19. Glossary

- **Capability** — a declarative data operation usable from the Operations menu, the DSL, and the AI tool layer.
- **Proposed Action** — a mutation the agent wants to apply; goes through user approval.
- **AgentEvent** — structured log event emitted by the agent loop; rendered in the terminal and used to drive UI updates.
- **Provider** — a configured way to reach an LLM (kind + model + base_url + credential ref).
- **CredentialStore** — local-only, OS-keyring-first, encrypted-blob-fallback secret store.
- **Answer-only mode** — setting that disables `propose_action`; agent can describe but not request mutations.

---

*End of plan.*
