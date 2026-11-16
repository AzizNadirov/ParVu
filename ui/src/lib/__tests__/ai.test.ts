import { describe, expect, it } from "vitest";
import {
  PROVIDER_PRESETS,
  applyEvent,
  emptyChat,
  finishRun,
  looksLikeKey,
  providerFromPreset,
  providerProblem,
  startRun,
  toolCallLabel,
  usageLabel,
} from "../ai";

describe("AI assistant logic", () => {
  it("builds a transcript from streamed events", () => {
    let s = startRun(emptyChat(), "North total?", 7);
    s = applyEvent(s, { type: "delta", delta: { type: "thinking", text: "Sum " } });
    s = applyEvent(s, { type: "delta", delta: { type: "thinking", text: "qty." } });
    s = applyEvent(s, { type: "delta", delta: { type: "tool_start", id: "t1", name: "query" } });
    s = applyEvent(s, { type: "tool_call", id: "t1", name: "query", input: { sql: "SELECT 1" } });
    s = applyEvent(s, { type: "tool_result", id: "t1", preview: "[[15]]", is_error: false });
    s = applyEvent(s, { type: "delta", delta: { type: "text", text: "North sold " } });
    s = applyEvent(s, { type: "delta", delta: { type: "text", text: "15." } });
    s = applyEvent(s, { type: "usage", usage: { input_tokens: 1000, output_tokens: 200, cache_read_tokens: 0, cache_write_tokens: 0 }, model: "m", cost_usd: 0.008 });
    s = applyEvent(s, { type: "done", stop: { type: "end_turn" } });
    s = finishRun(s, null, false);

    expect(s.runId).toBeNull();
    const a = s.items[1];
    expect(a.kind).toBe("assistant");
    if (a.kind !== "assistant") return;
    expect(a.done).toBe(true);
    expect(a.parts).toEqual([
      { type: "thinking", text: "Sum qty." },
      { type: "tool", id: "t1", name: "query", input: { sql: "SELECT 1" }, result: "[[15]]", isError: false },
      { type: "text", text: "North sold 15." },
    ]);
    expect(usageLabel(s)).toBe("1.2k tokens · $0.0080");
    // A model without a price makes the total unknown.
    s = applyEvent(s, { type: "usage", usage: { input_tokens: 1, output_tokens: 1, cache_read_tokens: 0, cache_write_tokens: 0 }, model: "x", cost_usd: null });
    expect(usageLabel(s)).toBe("1.2k tokens");
  });

  it("keeps errors and cancellations on the reply", () => {
    let s = finishRun(startRun(emptyChat(), "q", 1), "anthropic error (401): bad key", false);
    const a = s.items[1];
    expect(a.kind === "assistant" && a.error).toBe("anthropic error (401): bad key");
    s = finishRun(startRun(s, "q2", 2), null, true);
    const b = s.items[3];
    expect(b.kind === "assistant" && b.cancelled).toBe(true);
  });

  it("provider presets and checks", () => {
    const anthropic = providerFromPreset(PROVIDER_PRESETS[0], ["anthropic"]);
    expect(anthropic).toMatchObject({ id: "anthropic-2", kind: "anthropic", model: "claude-opus-5-5", server_fallback: true });
    expect(providerProblem(anthropic)).toBeNull();
    const ollama = providerFromPreset(PROVIDER_PRESETS.find((p) => p.key === "ollama")!, []);
    expect(ollama.base_url).toBe("http://localhost:11434/v1");
    expect(providerProblem(ollama)).toBe("ai.providers.need_model");
    expect(providerProblem({ ...ollama, model: "llama3.1", base_url: "" })).toBe("ai.providers.need_url");
    expect(providerProblem({ ...anthropic, id: "bad id" })).toBe("ai.providers.bad_id");
  });

  it("labels and key detection", () => {
    expect(toolCallLabel("query", { sql: "SELECT 1" })).toBe('query(sql: "SELECT 1")');
    expect(looksLikeKey("my key sk-ant-api03-abcdef123456")).toBe(true);
    expect(looksLikeKey("SELECT * FROM sk")).toBe(false);
  });
});
