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

import { AiPanel, SafeMarkdown } from "../AiPanel";

const ui = {
  t: (k: string, args?: Record<string, unknown>) => (args ? `${k}${JSON.stringify(args)}` : k),
  alert: vi.fn(async () => undefined),
  confirm: vi.fn(async () => false),
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
});
