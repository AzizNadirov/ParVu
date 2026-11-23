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

import {
  answerText,
  isSqlBlock,
  markdownSegments,
  quoteIdent,
  sqlEditBlocker,
  sqlIdentifiers,
  tableToTsv,
  toolResultCopyText,
  toolResultSql,
  toolResultTable,
} from "../ai";
import { sqlSpans } from "../sqlHighlight";

describe("AI results and SQL actions", () => {
  const rows = JSON.stringify({
    columns: ["region", "note"],
    types: ["VARCHAR", "VARCHAR"],
    rows: [["north", "a\tb"], ["south", null]],
    truncated: true,
  });

  it("reads tool results as tables", () => {
    expect(toolResultTable("query", rows)).toEqual({
      columns: ["region", "note"],
      rows: [["north", "a\tb"], ["south", null]],
      truncated: true,
    });
    const described = JSON.stringify({ table: "sales", rows: 6, columns: [{ name: "id", type: "INTEGER" }], steps: [] });
    expect(toolResultTable("describe_table", described)?.rows).toEqual([["id", "INTEGER"]]);
    const stats = JSON.stringify({ column: "qty", row_count: 6, mean: 4.25, min: null });
    expect(toolResultTable("column_stats", stats)?.rows).toEqual([["column", "qty"], ["row_count", "6"], ["mean", "4.25"], ["min", null]]);
    // Cut short for the model, an error, or not JSON: no table.
    expect(toolResultTable("query", rows.slice(0, 30) + " …[truncated]")).toBeNull();
    expect(toolResultTable("query", "Unknown table \"x\".")).toBeNull();
    expect(toolResultTable("query", undefined)).toBeNull();
  });

  it("copies results as TSV, or the text as it is", () => {
    const t = toolResultTable("query", rows)!;
    expect(tableToTsv(t)).toBe("region\tnote\nnorth\ta b\nsouth\t");
    expect(toolResultCopyText("query", rows)).toBe("region\tnote\nnorth\ta b\nsouth\t");
    expect(toolResultCopyText("undo", "Undone.")).toBe("Undone.");
  });

  it("builds SQL for results that can open in a new tab", () => {
    expect(toolResultSql("query", { sql: " SELECT 1 ", max_rows: 10 })).toBe("SELECT 1");
    expect(toolResultSql("query", { sql: "" })).toBeNull();
    expect(toolResultSql("value_counts", { table: 'my "t"', column: "Region", k: 5 })).toBe(
      'SELECT "Region" AS value, COUNT(*) AS count FROM "my ""t""" GROUP BY ALL ORDER BY count DESC, value',
    );
    expect(toolResultSql("sample_rows", { table: "sales", n: 5 })).toBe('SELECT * FROM "sales" LIMIT 5');
    expect(toolResultSql("sample_rows", { table: "sales", mode: "random", n: 500 })).toContain("reservoir(100 ROWS)");
    expect(toolResultSql("describe_table", { table: "sales" })).toBeNull();
    expect(quoteIdent('a"b')).toBe('"a""b"');
  });

  it("finds SQL code blocks", () => {
    expect(isSqlBlock("sql", "anything")).toBe(true);
    expect(isSqlBlock("DuckDB", "x")).toBe(true);
    expect(isSqlBlock("python", "select = 1")).toBe(false);
    expect(isSqlBlock("", "  WITH t AS (SELECT 1) SELECT * FROM t")).toBe(true);
    expect(isSqlBlock("", "FROM sales SELECT region")).toBe(true);
    expect(isSqlBlock(undefined, "region | total")).toBe(false);
    const segs = markdownSegments("Total by region:\n\n```sql\nSELECT region FROM sales\n```\n\nAnd:\n\n```python\nprint(1)\n```\n");
    expect(segs.map((s) => s.kind)).toEqual(["md", "code", "md", "code"]);
    expect(segs[1]).toEqual({ kind: "code", lang: "sql", text: "SELECT region FROM sales", sql: true });
    expect(segs[3]).toMatchObject({ kind: "code", lang: "python", sql: false });
    expect(segs[0]).toMatchObject({ kind: "md" });
    expect((segs[0] as { source: string }).source).toContain("Total by region:");
    // Raw HTML stays in the prose segment, for the safe renderer.
    expect(markdownSegments("<img src=x onerror=alert(1)>")).toEqual([{ kind: "md", source: "<img src=x onerror=alert(1)>" }]);
  });

  it("lets SQL into the editor only when its tables have no steps", () => {
    const tabs = [
      { name: "sales", steps: [{ key: "step.filter" }] },
      { name: "Regions", steps: [] },
    ];
    expect(sqlIdentifiers(`SELECT "Sales"."a b", x FROM t -- sales\nWHERE y = 'sales' /* sales */`)).toEqual([
      "select",
      "sales",
      "a b",
      "x",
      "from",
      "t",
      "where",
      "y",
    ]);
    expect(sqlEditBlocker("SELECT * FROM regions", tabs)).toBeNull();
    expect(sqlEditBlocker("SELECT * FROM SALES s JOIN regions USING (region)", tabs)).toBe("sales");
    expect(sqlEditBlocker('SELECT * FROM "sales"', tabs)).toBe("sales");
    expect(sqlEditBlocker("SELECT 'sales' AS name FROM regions", tabs)).toBeNull();
    expect(sqlEditBlocker("SELECT 1", [])).toBeNull();
  });

  it("copies an answer's text", () => {
    expect(
      answerText([
        { type: "thinking", text: "hmm" },
        { type: "text", text: "First.\n" },
        { type: "tool", id: "1", name: "query" },
        { type: "text", text: "```sql\nSELECT 1\n```" },
      ]),
    ).toBe("First.\n\n```sql\nSELECT 1\n```");
  });

  it("splits SQL into highlighted spans without losing text", () => {
    const sql = "SELECT region, 'x' AS s FROM sales -- note\nWHERE qty > 3";
    const spans = sqlSpans(sql);
    expect(spans.map((s) => s.text).join("")).toBe(sql);
    expect(spans.find((s) => s.text === "SELECT")?.cls).toContain("tok-keyword");
    expect(spans.find((s) => s.text === "'x'")?.cls).toContain("tok-string");
  });
});
