// Smoke tests: every dialog renders through a fake Ui and resolves with the
// expected value on submit / Escape.
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import type { Bootstrap, Settings } from "../../lib/api";
import type { Ui } from "../../lib/ui";
import { UiContext } from "../../lib/ui";

const invoke = vi.fn(async (cmd: string, _args?: Record<string, unknown>): Promise<unknown> => {
  switch (cmd) {
    case "list_themes":
      return [{ name: "ParVu Light", description: "d", author: "a", builtin: true }];
    case "theme_css":
      return ":root {\n  --pv-colors-background: #fff;\n}\n";
    case "preview_expression":
      return { sql: "SELECT *, a + b AS \"c\" FROM t", kind: { type: "scalar" } };
    case "tab_summary":
      return { pending_edits: 2 };
    case "find":
      return { row: 4, column_index: 1, column: "b", value: "hello" };
    default:
      return null;
  }
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(), openPath: vi.fn() }));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({ writeText: vi.fn(async () => undefined) }));

import * as D from "../index";

const settings = {
  default_data_var_name: "data",
  default_limit: "500",
  default_sql_font_size: "10",
  default_result_font_size: "11",
  default_sql_query: "SELECT 1",
  default_sql_font: "Courier",
  sql_keywords: ["SELECT"],
  result_pagination_rows_per_page: "500",
  save_file_history: "true",
  max_rows: "1000",
  current_theme: "ParVu Light",
  current_language: "en",
  enable_large_dataset_warning: true,
  warning_criteria: "rows",
  warn_on_exit_with_transforms: true,
  copy_tuple_sample_size: 500,
  warning_threshold_rows: 1_000_000,
  warning_threshold_cells: 10_000_000,
  warning_threshold_filesize_mb: 100,
  bug_report_email: "bugs@example.com",
  enable_crash_reporting: true,
} as Settings;

function makeUi(): Ui {
  const ui: Ui = {
    boot: {
      version: "1.0.0",
      languages: [{ code: "en", name: "English", native_name: "English", flag: "" }],
      language: "en",
      theme: { name: "ParVu Light", css: "" },
      functions: [
        {
          name: "UPPER",
          params: [{ name: "s", logical_type: "TEXT", required: true, description: "" }],
          return_type: "TEXT",
          description: "Upper case",
          example: "UPPER(data[a])",
          sql_name: "upper",
        },
      ],
      methods: { TEXT: ["upper"] },
      cast_types: ["", "VARCHAR"],
      parquet_compressions: ["snappy", "zstd"],
      log_dir: "/tmp/logs",
    } as unknown as Bootstrap,
    t: (k, args) => (args ? `${k}${JSON.stringify(args)}` : k),
    settings,
    setSettings: () => undefined,
    showDialog<T>(renderFn: (close: (v: T) => void) => ReactNode): Promise<T> {
      return new Promise<T>((resolve) => {
        const close = (v: T) => {
          resolve(v);
        };
        render(<UiContext.Provider value={ui}>{renderFn(close)}</UiContext.Provider>);
      });
    },
    alert: vi.fn(async () => undefined),
    ask: vi.fn(async () => "cancel"),
    confirm: vi.fn(async () => true),
    prompt: vi.fn(async () => null),
    status: vi.fn(),
  };
  return ui;
}

const escape = () => fireEvent.keyDown(document.querySelector(".dialog")!, { key: "Escape" });
const flush = () => act(async () => new Promise((r) => setTimeout(r, 5)));

afterEach(() => {
  cleanup();
  invoke.mockClear();
});

describe("dialogs render and resolve", () => {
  it("info dialogs close", async () => {
    const ui = makeUi();
    for (const open of [
      () => D.openAboutDialog(ui),
      () => D.openShortcutsDialog(ui),
      () => D.openExpressionHelp(ui),
      () => D.openCrashReport(ui, { report: "boom" }),
      () =>
        D.openColumnStats(ui, {
          stats: { column: "a", dtype: "INT", row_count: 2, non_null: 2, null: 0, distinct: 2, min: "1", max: "2", mean: 1.5, std: 0.5 },
        }),
      () =>
        D.openTableInfo(ui, {
          summary: { id: 1, name: "t", path: "/x.csv", format: "csv", file_size: 10, total_rows: 2, total_pages: 1, page_size: 500 } as never,
          columns: [{ name: "a", dtype: "INT" }],
        }),
    ]) {
      const p = open();
      await flush();
      escape();
      await expect(p).resolves.toBeUndefined();
      cleanup();
    }
  });

  it("drop null submits with Enter", async () => {
    const ui = makeUi();
    const p = D.openDropNull(ui, { table: "t", columns: ["a", "b"], selected: "b" });
    await flush();
    fireEvent.click(screen.getByText("dialog.drop_null.use_sentinel"));
    const input = screen.getByPlaceholderText("dialog.drop_null.value_placeholder");
    fireEvent.change(input, { target: { value: "-1" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await expect(p).resolves.toEqual({ op: "drop_null", column: "b", null_value: "-1" });
  });

  it("join, append, drop duplicates, replace", async () => {
    const ui = makeUi();
    let p: Promise<unknown> = D.openJoinDialog(ui, { table: "a", columns: ["id"], others: [{ name: "b", columns: ["x", "id"] }] });
    await flush();
    fireEvent.click(screen.getByText("btn.ok"));
    await expect(p).resolves.toEqual({ op: "join", right: "b", join_type: "inner", left_key: "id", right_key: "id" });
    cleanup();

    p = D.openAppendDialog(ui, { table: "a", others: ["b", "c"] });
    await flush();
    expect((screen.getByText("btn.ok") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByText("c"));
    fireEvent.click(screen.getByText("dialog.append.union"));
    fireEvent.click(screen.getByText("btn.ok"));
    await expect(p).resolves.toEqual({ op: "append", tables: ["c"], distinct: true });
    cleanup();

    p = D.openDropDuplicates(ui, { table: "a", columns: ["x", "y"] });
    await flush();
    fireEvent.click(screen.getByText("y"));
    fireEvent.click(screen.getByText("btn.ok"));
    await expect(p).resolves.toEqual({ op: "drop_duplicates", columns: ["y"], keep: "first" });
    cleanup();

    p = D.openReplace(ui, { table: "a", columns: ["x"], selected: "x" });
    await flush();
    const find = screen.getByPlaceholderText("dialog.replace.find_placeholder");
    expect(document.activeElement).toBe(find);
    fireEvent.change(find, { target: { value: "foo" } });
    fireEvent.keyDown(find, { key: "Enter" });
    await expect(p).resolves.toMatchObject({ column: "x", pattern: "foo", output_column: "x_replaced" });
  });

  it("unique values returns the selection, NULL included", async () => {
    const ui = makeUi();
    const p = D.openUniqueValues(ui, { column: "c", values: ["b", null, "a"] });
    await flush();
    fireEvent.click(screen.getByText("NULL"));
    fireEvent.click(screen.getByText("b"));
    fireEvent.click(screen.getByText("btn.apply_filter"));
    await expect(p).resolves.toEqual([null, "b"]);
  });

  it("copy tuple, unsaved changes, confirm close", async () => {
    const ui = makeUi();
    let p: Promise<unknown> = D.openCopyTuple(ui, { column: "c", totalRows: 50, pageSize: 10, defaultSample: 500 });
    await flush();
    fireEvent.click(screen.getByText("dialog.copy_tuple.random_n"));
    fireEvent.click(screen.getByText("dialog.copy_tuple.copy"));
    await expect(p).resolves.toEqual({ mode: "random", n: 50, label: "" });
    cleanup();

    p = D.openUnsavedChanges(ui, { count: 3 });
    await flush();
    escape();
    await expect(p).resolves.toBe("cancel");
    cleanup();

    p = D.openConfirmClose(ui, { tabs: ["a"] });
    await flush();
    fireEvent.click(screen.getByText("dialog.confirm_close.dont_ask"));
    fireEvent.click(screen.getByText("dialog.confirm_close.close_anyway"));
    await expect(p).resolves.toEqual({ result: "close", dontAsk: true });
  });

  it("math dialog previews and returns name + text", async () => {
    const ui = makeUi();
    const p = D.openMathDialog(ui, { tabId: 1, table: "t", columns: ["a", "b"] });
    await act(async () => new Promise((r) => setTimeout(r, 350)));
    expect(invoke).toHaveBeenCalledWith("preview_expression", { id: 1, text: "t[a] + t[b]" });
    expect(screen.getByText('SELECT *, a + b AS "c" FROM t')).toBeTruthy();
    const name = screen.getByPlaceholderText("dialog.math.column_placeholder");
    fireEvent.change(name, { target: { value: "total" } });
    fireEvent.keyDown(name, { key: "Enter" });
    await expect(p).resolves.toEqual({ name: "total", text: "t[a] + t[b]" });
  });

  it("settings, theme and language dialogs render", async () => {
    const ui = makeUi();
    const onSettings = vi.fn();
    let p: Promise<void> = D.openSettingsDialog(ui, { onSettings, onTheme: vi.fn(), onLanguage: vi.fn() });
    await flush();
    for (const tab of ["settings.tab.theme", "settings.tab.advanced", "settings.tab.warnings", "settings.tab.general"]) {
      fireEvent.click(screen.getByText(tab));
      await flush();
    }
    invoke.mockImplementationOnce(async () => settings);
    fireEvent.click(screen.getByText("btn.save"));
    await p;
    expect(invoke).toHaveBeenCalledWith("update_settings", expect.anything());
    expect(onSettings).toHaveBeenCalled();
    cleanup();

    p = D.openThemeSelector(ui, { onTheme: vi.fn() });
    await flush();
    expect(screen.getAllByText("ParVu Light").length).toBeGreaterThan(0);
    escape();
    await p;
    cleanup();

    p = D.openLanguageSelector(ui, { onLanguage: vi.fn() });
    await flush();
    escape();
    await p;
  });

  it("export dialog closes with null before starting", async () => {
    const ui = makeUi();
    const p = D.openExportDialog(ui, { tabId: 1, tableName: "t", columns: [{ name: "d", dtype: "DATE" }], defaultDir: "/out" });
    await flush();
    expect((screen.getByDisplayValue("/out/t.csv") as HTMLInputElement).value).toBe("/out/t.csv");
    fireEvent.change(screen.getByLabelText("dialog.export.format"), { target: { value: "parquet" } });
    expect(screen.getByDisplayValue("/out/t.parquet")).toBeTruthy();
    escape();
    await expect(p).resolves.toBeNull();
  });
});

describe("SearchPanel", () => {
  it("searches from the anchor and jumps", async () => {
    const ui = makeUi();
    const onJump = vi.fn();
    let keys: { next(): void; prev(): void } | null = null;
    render(
      <UiContext.Provider value={ui}>
        <D.SearchPanel
          tabId={7}
          columns={["a", "b"]}
          initialQuery="hel"
          anchor={{ row: 2, col: 0 }}
          onJump={onJump}
          onClose={vi.fn()}
          registerKeys={(k) => (keys = k)}
        />
      </UiContext.Provider>,
    );
    const input = screen.getByPlaceholderText("search.placeholder");
    fireEvent.keyDown(input, { key: "Enter" });
    await flush();
    expect(invoke).toHaveBeenCalledWith("find", {
      id: 7,
      options: expect.objectContaining({ query: "hel", direction: "next", after_row: 2, after_col: 0, column: null }),
    });
    expect(onJump).toHaveBeenCalledWith(4, "b");
    await act(async () => keys!.next());
    await flush();
    expect(invoke).toHaveBeenLastCalledWith("find", {
      id: 7,
      options: expect.objectContaining({ after_row: 4, after_col: 1 }),
    });
  });
});
