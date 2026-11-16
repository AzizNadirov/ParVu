// Pure logic of the AI assistant panel: provider presets and turning the
// backend's streamed events into a transcript.
import type { AgentEvent, AiStop, AiUsage, ProviderConfig } from "./api";

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
