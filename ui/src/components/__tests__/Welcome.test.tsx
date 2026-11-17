import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Welcome } from "../Panels";

afterEach(cleanup);

const t = (k: string, a?: Record<string, unknown>) => (a?.count !== undefined ? `${k}:${a.count}` : k);

function hub(recents: string[], aiReady = false) {
  const cb = {
    onOpen: vi.fn(),
    onBrowse: vi.fn(),
    onBrowseWithOptions: vi.fn(),
    onAiAssistant: vi.fn(),
    onConnectAi: vi.fn(),
    onShortcuts: vi.fn(),
    onGuide: vi.fn(),
    onChanges: vi.fn(),
    onTheme: vi.fn(),
    onLanguage: vi.fn(),
  };
  render(<Welcome t={t as never} recents={recents} aiReady={aiReady} themeName="Material Light" languageName="English" {...cb} />);
  return cb;
}

describe("start screen", () => {
  it("shows the three steps when there are no recent files", () => {
    hub([]);
    expect(screen.getByText("welcome.steps_title")).toBeTruthy();
    expect(screen.getByText("welcome.step3")).toBeTruthy();
    expect(screen.queryByText("welcome.recent")).toBeNull();
    expect(screen.getByText("welcome.ai_add_model")).toBeTruthy();
  });

  it("lists five recent files, then all of them, name before folder", () => {
    const files = Array.from({ length: 7 }, (_, i) => `/data/dir${i}/file${i}.parquet`);
    const cb = hub(files, true);
    expect(document.querySelectorAll(".hub-row").length).toBe(5);
    fireEvent.click(screen.getByText("welcome.all_files:7"));
    expect(document.querySelectorAll(".hub-row").length).toBe(7);
    const first = document.querySelector(".hub-row")!;
    expect(first.querySelector(".hub-fmt")!.textContent).toBe("PARQUET");
    expect(first.querySelector(".hub-name")!.textContent).toBe("file0.parquet");
    expect(first.querySelector(".hub-dir")!.textContent).toBe("/data/dir0");
    fireEvent.click(first);
    expect(cb.onOpen).toHaveBeenCalledWith("/data/dir0/file0.parquet");
    expect(screen.getByText("welcome.ai_ready")).toBeTruthy();
  });

  it("opens a pasted path and wires every action", () => {
    const cb = hub([]);
    const input = screen.getByLabelText("welcome.path_placeholder");
    fireEvent.change(input, { target: { value: "  /tmp/x.csv " } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(cb.onOpen).toHaveBeenCalledWith("/tmp/x.csv");
    fireEvent.click(screen.getByText("welcome.open"));
    fireEvent.click(screen.getByText("menu.file.open_options"));
    fireEvent.click(screen.getByText("menu.ai.assistant"));
    fireEvent.click(screen.getByText("menu.ai.connect"));
    fireEvent.click(screen.getByText("menu.help.shortcuts"));
    fireEvent.click(screen.getByText("menu.help.expression"));
    fireEvent.click(screen.getByText("welcome.all_changes"));
    fireEvent.click(screen.getByText("Material Light"));
    fireEvent.click(screen.getByText("English"));
    for (const f of [cb.onBrowse, cb.onBrowseWithOptions, cb.onAiAssistant, cb.onConnectAi, cb.onShortcuts, cb.onGuide, cb.onChanges, cb.onTheme, cb.onLanguage]) {
      expect(f).toHaveBeenCalledTimes(1);
    }
  });
});
