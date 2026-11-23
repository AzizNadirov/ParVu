// The assistant panel: empty states, sending a question, and that model
// output is rendered as Markdown without running raw HTML.
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ProvidersInfo } from "../../lib/api";
import type { Ui } from "../../lib/ui";
import { applyEvent, emptyChat, finishRun, startRun } from "../../lib/ai";

const invoke = vi.fn(async (cmd: string, _args?: Record<string, unknown>): Promise<unknown> => (cmd === "ai_ask" ? 42 : null));
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(async () => undefined) }));
const writeText = vi.fn(async (_s: string) => undefined);
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({ writeText: (s: string) => writeText(s) }));

import { AiPanel, SafeMarkdown } from "../AiPanel";

const ui = {
  t: (k: string, args?: Record<string, unknown>) => (args ? `${k}${JSON.stringify(args)}` : k),
  alert: vi.fn(async () => undefined),
  confirm: vi.fn(async () => false),
  status: vi.fn(),
} as unknown as Ui;

const providers: ProvidersInfo = {
  keychain: true,
  providers: [
    {
      config: { id: "anthropic", kind: "anthropic", display_name: "Anthropic", model: "claude-opus-5-5", base_url: null, key_env: null, max_output_tokens: 16000, effort: "medium", server_fallback: true, request_timeout_s: 300 },
      key_source: "keychain",
      is_default: true,
    },
  ],
};

function panel(over: Partial<Parameters<typeof AiPanel>[0]> = {}) {
  const props = {
    ui,
    tabId: 1,
    tabName: "sales",
    chat: emptyChat(),
    onStart: vi.fn(),
    onClear: vi.fn(),
    providers,
    providerId: "anthropic",
    onProvider: vi.fn(),
    onOpenProviders: vi.fn(),
    onConnectClient: vi.fn(),
    onClose: vi.fn(),
    ...over,
  };
  render(<AiPanel {...props} />);
  return props;
}

afterEach(() => {
  cleanup();
  invoke.mockClear();
  writeText.mockClear();
});

describe("AiPanel", () => {
  it("offers to add a provider, or to connect Claude, when none is set up", () => {
    const p = panel({ providers: { providers: [], keychain: true }, providerId: null });
    fireEvent.click(screen.getByText("ai.panel.add_provider"));
    expect(p.onOpenProviders).toHaveBeenCalled();
    fireEvent.click(screen.getByText("menu.ai.connect"));
    expect(p.onConnectClient).toHaveBeenCalled();
  });

  it("sends a question with Enter and starts the run", async () => {
    const p = panel();
    expect(screen.getByText('ai.panel.empty{"table":"sales"}')).toBeTruthy();
    const box = screen.getByPlaceholderText("ai.panel.placeholder");
    fireEvent.change(box, { target: { value: "Which region sold most?" } });
    await act(async () => {
      fireEvent.keyDown(box, { key: "Enter" });
    });
    expect(invoke).toHaveBeenCalledWith("ai_ask", { provider_id: "anthropic", tab_id: 1, question: "Which region sold most?" });
    expect(p.onStart).toHaveBeenCalledWith("Which region sold most?", 42);
  });

  it("asks before sending something that looks like an API key", async () => {
    panel();
    const box = screen.getByPlaceholderText("ai.panel.placeholder");
    fireEvent.change(box, { target: { value: "use sk-ant-api03-abcdef123456" } });
    await act(async () => {
      fireEvent.click(screen.getByText("ai.panel.send"));
    });
    expect(ui.confirm).toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalledWith("ai_ask", expect.anything());
  });

  it("shows the reply, tool calls and a Stop button while running", async () => {
    let chat = startRun(emptyChat(), "q", 9);
    chat = applyEvent(chat, { type: "tool_call", id: "t", name: "query", input: { sql: "SELECT 1" } });
    chat = applyEvent(chat, { type: "delta", delta: { type: "text", text: "**15** rows" } });
    panel({ chat });
    expect(screen.getByText('query(sql: "SELECT 1")')).toBeTruthy();
    expect(document.querySelector(".ai-md strong")?.textContent).toBe("15");
    await act(async () => {
      fireEvent.click(screen.getByText("ai.panel.stop"));
    });
    expect(invoke).toHaveBeenCalledWith("ai_cancel", { run_id: 9, tab_id: 1 });
    cleanup();
    panel({ chat: finishRun(chat, null, false) });
    expect(screen.getByText("ai.panel.send")).toBeTruthy();
  });

  it("never turns model output into live HTML", () => {
    render(<SafeMarkdown source={'Hi <img src=x onerror="alert(1)"> <script>alert(2)</script> | a |\n|---|\n| 1 |'} />);
    const root = document.querySelector(".ai-md")!;
    expect(root.querySelector("img, script")).toBeNull();
    expect(root.textContent).toContain("<img src=x");
  });

  describe("actions on the assistant's SQL and results", () => {
    const sql = "SELECT region, COUNT(*) AS n FROM sales GROUP BY ALL";
    const result = JSON.stringify({ columns: ["region", "n"], types: ["VARCHAR", "BIGINT"], rows: [["north", "3"], ["south", null]], truncated: false });
    function answered(text = "Done.\n\n```sql\nSELECT 1 FROM regions\n```\n\n```python\nprint('<b>hi</b>')\n```") {
      let chat = startRun(emptyChat(), "q", 9);
      chat = applyEvent(chat, { type: "tool_call", id: "t", name: "query", input: { sql, max_rows: 100 } });
      chat = applyEvent(chat, { type: "tool_result", id: "t", preview: result, is_error: false });
      chat = applyEvent(chat, { type: "delta", delta: { type: "text", text } });
      return finishRun(chat, null, false);
    }
    const tab = (name: string, steps: number) => ({ name, steps: Array.from({ length: steps }, () => ({ key: "step.filter", args: {} })) }) as never;

    it("shows the full SQL and the result table, with working buttons", async () => {
      const actions = { tabs: [tab("sales", 0)], openSql: vi.fn(), editSql: vi.fn() };
      panel({ chat: answered(), actions });
      const details = document.querySelector("details.ai-tool") as HTMLDetailsElement;
      await act(async () => {
        details.open = true;
        details.dispatchEvent(new Event("toggle"));
      });
      expect(details.querySelector(".ai-sql")?.textContent).toBe(sql);
      expect(details.querySelector(".ai-sql .tok-keyword")).toBeTruthy();
      expect([...details.querySelectorAll("td")].map((c) => c.textContent)).toEqual(["north", "3", "south", "NULL"]);
      const within = (label: string) => [...details.querySelectorAll("button")].find((b) => b.textContent === label)!;
      await act(async () => {
        fireEvent.click(within("ai.action.copy_sql"));
      });
      expect(writeText).toHaveBeenLastCalledWith(sql);
      await act(async () => {
        fireEvent.click(within("ai.action.copy_result"));
      });
      expect(writeText).toHaveBeenLastCalledWith("region\tn\nnorth\t3\nsouth\t");
      fireEvent.click(within("ai.action.open_tab"));
      expect(actions.openSql).toHaveBeenCalledWith(sql);
      fireEvent.click(within("ai.action.edit_sql"));
      expect(actions.editSql).toHaveBeenCalledWith(sql);
    });

    it("shows that Open in new tab is running, and can't be clicked twice", async () => {
      let done: () => void = () => undefined;
      const openSql = vi.fn(() => new Promise<void>((r) => (done = r)));
      panel({ chat: answered(), actions: { tabs: [tab("sales", 0)], openSql, editSql: vi.fn() } });
      const details = document.querySelector("details.ai-tool") as HTMLDetailsElement;
      await act(async () => {
        details.open = true;
        details.dispatchEvent(new Event("toggle"));
      });
      const button = () => [...details.querySelectorAll("button")].find((b) => b.textContent?.startsWith("ai.action.open") || b.textContent === "ai.action.opening")!;
      await act(async () => {
        fireEvent.click(button());
      });
      expect(button().textContent).toBe("ai.action.opening");
      expect(button().disabled).toBe(true);
      expect(button().querySelector(".ai-spinner")).toBeTruthy();
      fireEvent.click(button());
      expect(openSql).toHaveBeenCalledTimes(1);
      await act(async () => {
        done();
      });
      expect(button().textContent).toBe("ai.action.open_tab");
      expect(button().disabled).toBe(false);
    });

    it("keeps Edit in SQL editor off when a table has steps", async () => {
      const actions = { tabs: [tab("sales", 1)], openSql: vi.fn(), editSql: vi.fn() };
      panel({ chat: answered(), actions });
      const details = document.querySelector("details.ai-tool") as HTMLDetailsElement;
      await act(async () => {
        details.open = true;
        details.dispatchEvent(new Event("toggle"));
      });
      const edit = [...details.querySelectorAll("button")].find((b) => b.textContent === "ai.action.edit_sql")!;
      expect(edit.disabled).toBe(true);
      expect(edit.parentElement?.getAttribute("title")).toBe('ai.action.edit_sql_blocked{"table":"sales"}');
    });

    it("adds buttons to code blocks and copies the whole answer, without live HTML", async () => {
      const actions = { tabs: [tab("sales", 1), tab("regions", 0)], openSql: vi.fn(), editSql: vi.fn() };
      panel({ chat: answered(), actions });
      const blocks = document.querySelectorAll(".ai-code");
      expect(blocks.length).toBe(2);
      const buttons = (el: Element) => [...el.querySelectorAll("button")].map((b) => b.textContent);
      expect(buttons(blocks[0])).toEqual(["ai.action.copy_sql", "ai.action.open_tab", "ai.action.edit_sql"]);
      expect(buttons(blocks[1])).toEqual(["ai.action.copy"]);
      expect(blocks[1].querySelector("b")).toBeNull();
      expect(blocks[1].textContent).toContain("<b>hi</b>");
      fireEvent.click([...blocks[0].querySelectorAll("button")].find((b) => b.textContent === "ai.action.edit_sql")!);
      expect(actions.editSql).toHaveBeenCalledWith("SELECT 1 FROM regions");
      await act(async () => {
        fireEvent.click(screen.getByLabelText("ai.action.copy_answer"));
      });
      expect(writeText).toHaveBeenLastCalledWith("Done.\n\n```sql\nSELECT 1 FROM regions\n```\n\n```python\nprint('<b>hi</b>')\n```");
    });
  });
});
