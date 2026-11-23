// Pure logic of the AI assistant panel: provider presets and turning the
// backend's streamed events into a transcript.
// @ts-ignore -- marked 4 ships no type declarations
import { marked } from "marked";
import type { AgentEvent, AiStop, AiUsage, Cell, ProviderConfig, SavedDisplay } from "./api";
import { formatRows } from "./copy";

export interface ProviderPreset {
  key: string;
  label: string;
  kind: ProviderConfig["kind"];
  base_url: string | null;
  key_env: string | null;
  /** Suggested model ids (empty: the user types one). */
  models: string[];
  needsKey: boolean;
}

export const PROVIDER_PRESETS: ProviderPreset[] = [
  { key: "anthropic", label: "Anthropic (Claude)", kind: "anthropic", base_url: null, key_env: "ANTHROPIC_API_KEY", models: ["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"], needsKey: true },
  { key: "openai", label: "OpenAI", kind: "open_ai_compatible", base_url: "https://api.openai.com/v1", key_env: "OPENAI_API_KEY", models: [], needsKey: true },
  { key: "gemini", label: "Google Gemini", kind: "open_ai_compatible", base_url: "https://generativelanguage.googleapis.com/v1beta/openai", key_env: "GEMINI_API_KEY", models: [], needsKey: true },
  { key: "openrouter", label: "OpenRouter", kind: "open_ai_compatible", base_url: "https://openrouter.ai/api/v1", key_env: "OPENROUTER_API_KEY", models: [], needsKey: true },
  { key: "groq", label: "Groq", kind: "open_ai_compatible", base_url: "https://api.groq.com/openai/v1", key_env: "GROQ_API_KEY", models: [], needsKey: true },
  { key: "ollama", label: "Ollama (local)", kind: "open_ai_compatible", base_url: "http://localhost:11434/v1", key_env: null, models: [], needsKey: false },
  { key: "lmstudio", label: "LM Studio (local)", kind: "open_ai_compatible", base_url: "http://localhost:1234/v1", key_env: null, models: [], needsKey: false },
  { key: "custom", label: "Other (OpenAI-compatible)", kind: "open_ai_compatible", base_url: "", key_env: null, models: [], needsKey: false },
];

/** A new provider config from a preset, with an id not in `taken`. */
export function providerFromPreset(p: ProviderPreset, taken: string[]): ProviderConfig {
  let id = p.key;
  for (let n = 2; taken.includes(id); n++) id = `${p.key}-${n}`;
  return {
    id,
    kind: p.kind,
    display_name: p.label,
    model: p.models[0] ?? "",
    base_url: p.base_url,
    key_env: p.key_env,
    max_output_tokens: 16000,
    effort: "medium",
    server_fallback: p.kind === "anthropic",
    request_timeout_s: 300,
  };
}

/** What a provider config still needs before it can be saved. */
export function providerProblem(c: ProviderConfig): string | null {
  if (!/^[A-Za-z0-9_-]+$/.test(c.id)) return "ai.providers.bad_id";
  if (!c.model.trim()) return "ai.providers.need_model";
  if (c.kind === "open_ai_compatible" && !(c.base_url ?? "").trim()) return "ai.providers.need_url";
  return null;
}

// ---------- transcript ----------

export type AssistantPart =
  | { type: "text"; text: string }
  | { type: "thinking"; text: string }
  | { type: "tool"; id: string; name: string; input?: unknown; result?: string; isError?: boolean }
  | { type: "fallback"; from: string; to: string };

export type ChatItem =
  | { kind: "user"; text: string }
  | { kind: "assistant"; parts: AssistantPart[]; done: boolean; stop?: AiStop; error?: string; cancelled?: boolean };

export interface ChatState {
  items: ChatItem[];
  runId: number | null;
  /** Totals for this chat. */
  usage: AiUsage;
  /** Dollars; null when any call had no known price. */
  cost: number | null;
  /** The first `count` items came back from the file's saved conversation. */
  restored?: { count: number; updatedMs: number };
}

export const emptyChat = (): ChatState => ({
  items: [],
  runId: null,
  usage: { input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0 },
  cost: 0,
});

/** The user asked a question; an empty assistant reply starts streaming. */
export function startRun(s: ChatState, question: string, runId: number): ChatState {
  return {
    ...s,
    runId,
    items: [...s.items, { kind: "user", text: question }, { kind: "assistant", parts: [], done: false }],
  };
}

function updateLast(s: ChatState, f: (a: Extract<ChatItem, { kind: "assistant" }>) => Extract<ChatItem, { kind: "assistant" }>): ChatState {
  const i = s.items.length - 1;
  const last = s.items[i];
  if (!last || last.kind !== "assistant") return s;
  const items = s.items.slice();
  items[i] = f(last);
  return { ...s, items };
}

function appendText(parts: AssistantPart[], type: "text" | "thinking", text: string): AssistantPart[] {
  const last = parts[parts.length - 1];
  if (last && last.type === type) return [...parts.slice(0, -1), { ...last, text: last.text + text }];
  return [...parts, { type, text }];
}

/** Apply one streamed event to the running reply. */
export function applyEvent(s: ChatState, ev: AgentEvent): ChatState {
  switch (ev.type) {
    case "delta": {
      const d = ev.delta;
      if (d.type === "text" || d.type === "thinking") return updateLast(s, (a) => ({ ...a, parts: appendText(a.parts, d.type, d.text) }));
      if (d.type === "fallback") return updateLast(s, (a) => ({ ...a, parts: [...a.parts, { type: "fallback", from: d.from, to: d.to }] }));
      return updateLast(s, (a) =>
        a.parts.some((p) => p.type === "tool" && p.id === d.id) ? a : { ...a, parts: [...a.parts, { type: "tool", id: d.id, name: d.name }] },
      );
    }
    case "tool_call":
      return updateLast(s, (a) => {
        const has = a.parts.some((p) => p.type === "tool" && p.id === ev.id);
        const parts = has
          ? a.parts.map((p) => (p.type === "tool" && p.id === ev.id ? { ...p, input: ev.input } : p))
          : [...a.parts, { type: "tool" as const, id: ev.id, name: ev.name, input: ev.input }];
        return { ...a, parts };
      });
    case "tool_result":
      return updateLast(s, (a) => ({
        ...a,
        parts: a.parts.map((p) => (p.type === "tool" && p.id === ev.id ? { ...p, result: ev.preview, isError: ev.is_error } : p)),
      }));
    case "usage": {
      const u = s.usage;
      const usage = {
        input_tokens: u.input_tokens + ev.usage.input_tokens,
        output_tokens: u.output_tokens + ev.usage.output_tokens,
        cache_read_tokens: u.cache_read_tokens + ev.usage.cache_read_tokens,
        cache_write_tokens: u.cache_write_tokens + ev.usage.cache_write_tokens,
      };
      const cost = s.cost === null || ev.cost_usd === null ? null : s.cost + ev.cost_usd;
      return { ...s, usage, cost };
    }
    case "done":
      return updateLast(s, (a) => ({ ...a, stop: ev.stop }));
  }
}

/** The run ended (normally, with an error, or cancelled). */
export function finishRun(s: ChatState, error: string | null, cancelled: boolean): ChatState {
  return {
    ...updateLast(s, (a) => ({ ...a, done: true, error: error ?? undefined, cancelled: cancelled || undefined })),
    runId: null,
  };
}

// ---------- saved conversations (per file, ai_assistant.rs `ai_save_chat`)

/** What is saved with the file: the finished items and the totals. */
export function chatToSave(s: ChatState): SavedDisplay {
  return {
    items: s.items.filter((it) => it.kind === "user" || it.done),
    usage: s.usage,
    cost: s.cost,
  };
}

const isObj = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);
const num = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? v : 0);

function restorePart(p: unknown): AssistantPart | null {
  if (!isObj(p)) return null;
  switch (p.type) {
    case "text":
    case "thinking":
      return typeof p.text === "string" ? { type: p.type, text: p.text } : null;
    case "fallback":
      return typeof p.from === "string" && typeof p.to === "string" ? { type: "fallback", from: p.from, to: p.to } : null;
    case "tool": {
      if (typeof p.id !== "string" || typeof p.name !== "string") return null;
      const tool: Extract<AssistantPart, { type: "tool" }> = { type: "tool", id: p.id, name: p.name };
      if (p.input !== undefined) tool.input = p.input;
      if (typeof p.result === "string") tool.result = p.result;
      if (typeof p.isError === "boolean") tool.isError = p.isError;
      return tool;
    }
  }
  return null;
}

function restoreItem(it: unknown): ChatItem | null {
  if (!isObj(it)) return null;
  if (it.kind === "user") return typeof it.text === "string" ? { kind: "user", text: it.text } : null;
  if (it.kind !== "assistant" || !Array.isArray(it.parts)) return null;
  const parts = it.parts.map(restorePart).filter((p): p is AssistantPart => p !== null);
  const item: Extract<ChatItem, { kind: "assistant" }> = { kind: "assistant", parts, done: true };
  if (isObj(it.stop) && typeof it.stop.type === "string") item.stop = it.stop as AiStop;
  if (typeof it.error === "string") item.error = it.error;
  // Saved while running: it can't go on.
  if (it.cancelled === true || it.done !== true) item.cancelled = true;
  return item;
}

/**
 * A chat from a saved transcript (`display` of ai_load_chat), or `null` when
 * it holds nothing to show. Anything malformed is skipped rather than
 * trusted: the file may come from another version of ParVu.
 */
export function restoreChat(display: unknown, updatedMs: number): ChatState | null {
  if (!isObj(display) || !Array.isArray(display.items)) return null;
  const items = display.items.map(restoreItem).filter((it): it is ChatItem => it !== null);
  if (!items.length) return null;
  const u = isObj(display.usage) ? display.usage : {};
  return {
    items,
    runId: null,
    usage: {
      input_tokens: num(u.input_tokens),
      output_tokens: num(u.output_tokens),
      cache_read_tokens: num(u.cache_read_tokens),
      cache_write_tokens: num(u.cache_write_tokens),
    },
    cost: display.cost === null ? null : num(display.cost),
    restored: { count: items.length, updatedMs },
  };
}

/**
 * Where the panel marks the restored conversation: the label row goes
 * before item `before` = 0, and a plain rule after the restored items
 * (`after`) once newer ones follow. `null` for a chat that wasn't restored.
 */
export function restoredDivider(s: ChatState): { updatedMs: number; after: number | null } | null {
  if (!s.restored || s.restored.count === 0) return null;
  const count = Math.min(s.restored.count, s.items.length);
  return { updatedMs: s.restored.updatedMs, after: count < s.items.length ? count : null };
}

/** "1.2k tokens · $0.0040" */
export function usageLabel(s: ChatState): string {
  const tokens = s.usage.input_tokens + s.usage.output_tokens + s.usage.cache_read_tokens + s.usage.cache_write_tokens;
  const t = tokens >= 1000 ? `${(tokens / 1000).toFixed(1)}k` : String(tokens);
  const cost = s.cost === null ? "" : ` · $${s.cost < 0.01 ? s.cost.toFixed(4) : s.cost.toFixed(2)}`;
  return `${t} tokens${cost}`;
}

/** Short one-line view of a tool input: `query(sql: "SELECT …")`. */
export function toolCallLabel(name: string, input: unknown): string {
  if (!input || typeof input !== "object") return name;
  const args = Object.entries(input as Record<string, unknown>)
    .map(([k, v]) => {
      const s = typeof v === "string" ? JSON.stringify(v) : JSON.stringify(v) ?? "";
      return `${k}: ${s.length > 60 ? `${s.slice(0, 59)}…` : s}`;
    })
    .join(", ");
  return `${name}(${args})`;
}

/** Whether text looks like it contains an API key (warn before sending). */
export function looksLikeKey(text: string): boolean {
  return /\b(sk-[A-Za-z0-9_-]{8,}|gsk_[A-Za-z0-9]{8,}|AIza[A-Za-z0-9_-]{8,})/.test(text);
}

// ---------------------------------------------------------------- actions on results and SQL

/** Rows of a tool result, for a small table and "Copy result". */
export interface ResultTable {
  columns: string[];
  rows: Cell[][];
  /** The tool returned only the first rows. */
  truncated: boolean;
}

const cellText = (v: unknown): Cell => (v === null || v === undefined ? null : typeof v === "string" ? v : JSON.stringify(v));

/**
 * A tool result as a table, when it is one: rows (`query`, `sample_rows`,
 * `value_counts`), the columns of `describe_table`, or the numbers of
 * `column_stats` as name/value pairs. `null` for anything else, including a
 * result cut short for the model (no longer valid JSON).
 */
export function toolResultTable(name: string, text: string | undefined): ResultTable | null {
  if (!text) return null;
  let v: unknown;
  try {
    v = JSON.parse(text);
  } catch {
    return null;
  }
  if (!v || typeof v !== "object" || Array.isArray(v)) return null;
  const o = v as Record<string, unknown>;
  if (Array.isArray(o.columns) && Array.isArray(o.rows) && o.columns.every((c) => typeof c === "string")) {
    return {
      columns: o.columns as string[],
      rows: (o.rows as unknown[]).map((r) => (Array.isArray(r) ? r.map(cellText) : [cellText(r)])),
      truncated: o.truncated === true,
    };
  }
  if (name === "describe_table" && Array.isArray(o.columns)) {
    return {
      columns: ["column", "type"],
      rows: (o.columns as Record<string, unknown>[]).map((c) => [cellText(c?.name), cellText(c?.type)]),
      truncated: false,
    };
  }
  if (name === "column_stats") {
    return { columns: ["statistic", "value"], rows: Object.entries(o).map(([k, x]) => [k, cellText(x)]), truncated: false };
  }
  return null;
}

/** A result table as tab-separated text with a header (pastes into a spreadsheet). */
export function tableToTsv(t: ResultTable): string {
  return formatRows(t.rows, "tsv", t.columns);
}

/** What "Copy result" copies: the table as TSV, or the text as it is. */
export function toolResultCopyText(name: string, text: string): string {
  const t = toolResultTable(name, text);
  return t ? tableToTsv(t) : text;
}

/** `name` as a SQL identifier. */
export function quoteIdent(name: string): string {
  return `"${name.replace(/"/g, '""')}"`;
}

const str = (v: unknown) => (typeof v === "string" ? v : "");

/**
 * SQL that gives a tool call's result again (without the row cap for
 * `query` and all values for `value_counts`), for "Open result in a new
 * tab"; `null` for tools without one. Table names mean each tab's current
 * result, as in the assistant's tools.
 */
export function toolResultSql(name: string, input: unknown): string | null {
  const a = (input && typeof input === "object" ? input : {}) as Record<string, unknown>;
  switch (name) {
    case "query":
      return str(a.sql).trim() || null;
    case "value_counts": {
      if (!str(a.table) || !str(a.column)) return null;
      return `SELECT ${quoteIdent(str(a.column))} AS value, COUNT(*) AS count FROM ${quoteIdent(str(a.table))} GROUP BY ALL ORDER BY count DESC, value`;
    }
    case "sample_rows": {
      if (!str(a.table)) return null;
      const n = Math.min(100, Math.max(1, Math.floor(typeof a.n === "number" ? a.n : 10)));
      const t = quoteIdent(str(a.table));
      switch (a.mode ?? "head") {
        case "head":
          return `SELECT * FROM ${t} LIMIT ${n}`;
        case "tail":
          return `SELECT * EXCLUDE (__pv_n) FROM (SELECT *, ROW_NUMBER() OVER () AS __pv_n FROM ${t}) QUALIFY __pv_n > COUNT(*) OVER () - ${n} ORDER BY __pv_n`;
        case "random":
          return `SELECT * FROM ${t} USING SAMPLE reservoir(${n} ROWS) REPEATABLE (42)`;
      }
      return null;
    }
  }
  return null;
}

/** Whether a Markdown code block holds SQL: tagged sql (or a SQL dialect), or untagged and starting like a query. */
export function isSqlBlock(lang: string | undefined, text: string): boolean {
  const l = (lang ?? "").trim().split(/\s+/)[0].toLowerCase();
  if (l) return ["sql", "duckdb", "postgresql", "postgres", "pgsql", "psql"].includes(l);
  return /^\s*(select|with|from)\b/i.test(text);
}

/** Markdown split into prose and the top-level code blocks, which get their own buttons. */
export type MdSegment = { kind: "md"; source: string } | { kind: "code"; lang: string; text: string; sql: boolean };

type MdToken = { type: string; raw: string; lang?: string; text?: string };

export function markdownSegments(source: string): MdSegment[] {
  let tokens: MdToken[];
  try {
    tokens = (marked as unknown as { lexer(s: string): MdToken[] }).lexer(source);
  } catch {
    return [{ kind: "md", source }];
  }
  const out: MdSegment[] = [];
  for (const tok of tokens) {
    if (tok.type === "code") {
      const lang = (tok.lang ?? "").trim();
      const text = tok.text ?? "";
      out.push({ kind: "code", lang, text, sql: isSqlBlock(lang, text) });
      continue;
    }
    const last = out[out.length - 1];
    if (last && last.kind === "md") last.source += tok.raw;
    else out.push({ kind: "md", source: tok.raw });
  }
  return out.filter((s) => s.kind === "code" || s.source.trim() !== "");
}

/** Identifiers in `sql`, lower-cased (quoted ones unquoted), skipping strings and comments. */
export function sqlIdentifiers(sql: string): string[] {
  const out: string[] = [];
  const re = /'(?:[^']|'')*'?|--[^\n]*|\/\*[\s\S]*?(?:\*\/|$)|"((?:[^"]|"")*)"?|([A-Za-z_][A-Za-z0-9_$]*)/g;
  for (let m = re.exec(sql); m; m = re.exec(sql)) {
    if (m[1] !== undefined) out.push(m[1].replace(/""/g, '"').toLowerCase());
    else if (m[2] !== undefined) out.push(m[2].toLowerCase());
  }
  return out;
}

/**
 * Why the assistant's SQL can't go into the SQL editor as it is, or `null`
 * when it can. The assistant's queries see each table as the tab's current
 * result (steps applied), while SQL mode reads the original file, so a
 * query that names a tab with steps would give other numbers there. Names
 * are matched anywhere in the query (a column of the same name counts too),
 * which errs on the safe side. Returns the first such tab's name.
 */
export function sqlEditBlocker(sql: string, tabs: { name: string; steps: unknown[] }[]): string | null {
  const used = new Set(sqlIdentifiers(sql));
  return tabs.find((t) => t.steps.length > 0 && used.has(t.name.toLowerCase()))?.name ?? null;
}

/** All of an answer's text (its prose and code blocks, not tool calls), for "Copy answer". */
export function answerText(parts: AssistantPart[]): string {
  return parts
    .filter((p): p is Extract<AssistantPart, { type: "text" }> => p.type === "text")
    .map((p) => p.text.trim())
    .filter(Boolean)
    .join("\n\n");
}
