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
    case "ai_providers":
    case "ai_save_provider":
      return { providers: cmd === "ai_save_provider" ? [{ config: (_args as { config: unknown }).config, key_source: "keychain", is_default: true }] : [], keychain: true };
    case "ai_test_provider":
      return "claude-opus-5-5";
    case "chart_data":
      return {
        x_kind: "category", time_unit: null, labels: ["north", "south", "Other"], other_label: 2, bin_edges: [], truncated: false,
        series: [{ name: "", other: false, values: [15, 2, 4], xs: [] }],
      };
    case "suggest_reading":
      return { kind: "locale_number", thousands: ".", decimal: "," };
    case "preview_conversion":
      return [
        { before: "1.234,5", after: "1234.5", failed: false },
        { before: "n/a", after: null, failed: true },
      ];
    case "conversion_error_count":
      return { errors: 1, checked: 2, complete: true };
    case "mcp_status":
    case "mcp_set_enabled":
      return { enabled: cmd === "mcp_set_enabled", listening: cmd === "mcp_set_enabled", port: 4000, clients: 0, command: "parvu", args: ["mcp", "--attach"], error: null };
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

describe("pandas-style operation dialogs", () => {
  it("submit their operation", async () => {
    const ui = makeUi();
    const columns = [
      { name: "region", dtype: "VARCHAR" },
      { name: "qty", dtype: "INTEGER" },
    ];
    const submit = async (p: Promise<unknown>) => {
      await flush();
      fireEvent.click(screen.getByText("btn.apply"));
      const v = await p;
      cleanup();
      return v;
    };
    expect(await submit(D.openGroupBy(ui, { table: "t", columns, selected: "region" }))).toEqual({
      op: "group_by",
      by: ["region"],
      aggregations: [
        { column: "", func: "rows", alias: null },
        { column: "qty", func: "sum", alias: null },
      ],
    });
    expect(await submit(D.openPivot(ui, { table: "t", columns, selected: "region" }))).toEqual({
      op: "pivot",
      index: [],
      columns: "region",
      values: "qty",
      func: "sum",
    });
    expect(await submit(D.openMelt(ui, { table: "t", columns }))).toMatchObject({ op: "melt", id_columns: ["region"] });
    expect(await submit(D.openTranspose(ui, { table: "t", columns, rows: 3 }))).toEqual({ op: "transpose", header_column: null });
    expect(await submit(D.openValueCounts(ui, { table: "t", columns, selected: "qty" }))).toMatchObject({ columns: ["qty"] });
    expect(await submit(D.openFilter(ui, { table: "t", columns, selected: "qty", initial: { op: "gt", value: "2" } }))).toEqual({
      op: "filter",
      conditions: [{ column: "qty", op: "gt", value: "2", value2: "", case_sensitive: false }],
      combine: "and",
    });
    const fill = D.openFillNull(ui, { table: "t", columns, selected: "qty" });
    await flush();
    expect((screen.getByText("btn.apply") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(screen.getByDisplayValue("fill.value"), { target: { value: "forward" } });
    fireEvent.click(screen.getByText("btn.apply"));
    expect(await fill).toEqual({ op: "fill_null", columns: ["qty"], method: "forward", value: "" });
    cleanup();
    expect(await submit(D.openSort(ui, { table: "t", columns, selected: "qty" }))).toEqual({
      op: "sort_by",
      keys: [{ column: "qty", ascending: true, nulls_first: false }],
    });
    expect(await submit(D.openSlice(ui, { table: "t", rows: 10 }))).toEqual({ op: "slice", mode: "head", n: 100, seed: null });
    expect(await submit(D.openSplitColumn(ui, { table: "t", columns }))).toMatchObject({ column: "region", separator: "," });
    expect(await submit(D.openBin(ui, { table: "t", columns }))).toMatchObject({ column: "qty", output: "qty_bin", bins: 5 });
    expect(await submit(D.openWindow(ui, { table: "t", columns }))).toMatchObject({ op: "window", func: "cum_sum", column: "qty", output: "qty_cumsum" });
  });

  it("the transpose dialog refuses big tables", async () => {
    const ui = makeUi();
    void D.openTranspose(ui, { table: "t", columns: [{ name: "a", dtype: "INT" }], rows: 501 });
    await flush();
    expect((screen.getByText("btn.apply") as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("spreadsheet dialogs", () => {
  it("conditional formatting returns the edited rules", async () => {
    const ui = makeUi();
    const p = D.openConditionalFormat(ui, {
      columns: [{ name: "qty", dtype: "INTEGER" }],
      rules: [{ kind: "scale", column: "qty", color: "primary" }],
      selected: "qty",
    });
    await flush();
    fireEvent.click(screen.getByText("dialog.format.highlight"));
    fireEvent.change(document.querySelector(".pv-indent input")!, { target: { value: "5" } });
    fireEvent.click(screen.getByText("dialog.format.add_rule"));
    fireEvent.click(screen.getByText("btn.ok"));
    await expect(p).resolves.toEqual([
      { kind: "scale", column: "qty", color: "primary" },
      { kind: "highlight", column: "qty", op: "gt", value: "5", value2: "", color: "primary" },
    ]);
  });

  it("the chart dialog asks DuckDB for chart data and closes", async () => {
    const ui = makeUi();
    const p = D.openChart(ui, {
      tabId: 3,
      table: "t",
      columns: [
        { name: "region", dtype: "VARCHAR" },
        { name: "qty", dtype: "INTEGER" },
        { name: "product", dtype: "VARCHAR" },
      ],
    });
    await act(async () => new Promise((r) => setTimeout(r, 400)));
    expect(invoke).toHaveBeenCalledWith("chart_data", {
      id: 3,
      spec: { kind: "bar", x: "region", y: "qty", agg: "sum", series: null, time_unit: "auto", sort: "auto", limit: 20, other: true, bins: 20 },
    });
    // The chart is drawn: one bar per category, the generated title, no
    // legend for a single series.
    const svg = document.querySelector("svg.pv-chart")!;
    expect(svg.querySelectorAll("path[d]").length).toBeGreaterThanOrEqual(2);
    expect(svg.textContent).toContain("chart.title.by");
    expect(svg.textContent).toContain("north");
    // Split by a column: the request carries it.
    fireEvent.change(screen.getAllByRole("combobox")[2], { target: { value: "region" } });
    await act(async () => new Promise((r) => setTimeout(r, 400)));
    fireEvent.click(screen.getByText("dialog.chart.table_view"));
    expect(document.querySelector(".pv-chart-table td")?.textContent).toBe("north");
    fireEvent.click(screen.getByText("btn.close"));
    await expect(p).resolves.toBeUndefined();
  });
});


describe("Convert", () => {
  const eu = { kind: "locale_number", thousands: ".", decimal: "," };

  it("starts from the suggested reading, previews, counts and converts", async () => {
    const ui = makeUi();
    const p = D.openConvert(ui, { tabId: 2, table: "t", column: "amount" });
    await flush(); // the suggestion arrives and is selected
    await act(async () => new Promise((r) => setTimeout(r, 300))); // then the debounced preview
    expect(invoke).toHaveBeenCalledWith("suggest_reading", { id: 2, column: "amount" });
    expect(invoke).toHaveBeenCalledWith("preview_conversion", { id: 2, column: "amount", to: eu, n: 50 });
    expect(invoke).toHaveBeenCalledWith("conversion_error_count", { id: 2, column: "amount", to: eu, full: false });
    expect(document.querySelector(".pv-convert-failed")?.textContent).toContain("n/a");
    expect(screen.getByText(/dialog\.convert\.errors_all/)).toBeTruthy();
    fireEvent.click(screen.getByText("dialog.convert.keep_original", { exact: false }));
    fireEvent.click(screen.getByText("dialog.convert.apply"));
    await expect(p).resolves.toEqual({ op: "convert", column: "amount", to: eu, on_error: null, keep_original: true });
  });

  it("shows the rows that don't convert", async () => {
    const ui = makeUi();
    const p = D.openConvert(ui, { tabId: 2, table: "t", column: "amount" });
    await flush();
    await act(async () => new Promise((r) => setTimeout(r, 300)));
    fireEvent.click(screen.getByText("dialog.convert.show_errors"));
    await expect(p).resolves.toEqual({ op: "conversion_errors", column: "amount", to: eu, keep: true });
  });
});

describe("AI clients (MCP)", () => {
  it("connect dialog shows setup commands and switches the listener", async () => {
    const ui = makeUi();
    const p = D.openConnectAi(ui);
    await flush();
    expect(screen.getByText("claude mcp add -s user parvu-app -- parvu mcp --attach")).toBeTruthy();
    expect(screen.getByText("dialog.connect_ai.off")).toBeTruthy();
    fireEvent.click(screen.getByRole("checkbox"));
    await flush();
    expect(invoke).toHaveBeenCalledWith("mcp_set_enabled", { enabled: true });
    expect(screen.getByText('dialog.connect_ai.listening{"count":0}')).toBeTruthy();
    fireEvent.click(screen.getByText("btn.close"));
    await expect(p).resolves.toBeUndefined();
  });

  it("proposal card shows the change and resolves with the answer", async () => {
    const proposal = {
      id: 7,
      window: "main",
      client: "Claude Code",
      tab_id: 1,
      table: "sales",
      kind: "operation" as const,
      label: { key: "step.remove", args: { column: "qty" } },
      rationale: "qty is unused",
      sql: "SELECT * EXCLUDE (qty) FROM sales",
      rows_before: 3,
      rows_after: 3,
      columns: [{ name: "id", dtype: "BIGINT" }],
      rows: [["1"], [null]],
    };
    const ui = makeUi();
    let p = D.openMcpProposal(ui, proposal);
    await flush();
    expect(screen.getByText('mcp.proposal.title{"client":"Claude Code","table":"sales"}')).toBeTruthy();
    expect(screen.getByText('mcp.proposal.why{"rationale":"qty is unused"}')).toBeTruthy();
    expect(screen.getByText("NULL")).toBeTruthy();
    fireEvent.click(screen.getByText("mcp.proposal.apply"));
    await expect(p).resolves.toBe(true);
    cleanup();

    p = D.openMcpProposal(ui, { ...proposal, kind: "undo", sql: null, rows_after: null, columns: [], rows: [] });
    await flush();
    expect(screen.getByText("mcp.proposal.undo")).toBeTruthy();
    escape();
    await expect(p).resolves.toBe(false);
  });
});

describe("AI assistant", () => {
  it("providers dialog adds an Anthropic provider, stores the key and tests it", async () => {
    const ui = makeUi();
    const p = D.openAiProviders(ui);
    await flush();
    expect(screen.getByText("ai.providers.none")).toBeTruthy();
    fireEvent.click(screen.getByText("ai.providers.add"));
    await flush();
    fireEvent.change(document.querySelector('input[type="password"]')!, { target: { value: "sk-ant-test-123456789" } });
    fireEvent.click(screen.getByText("ai.providers.save_test"));
    await flush();
    await flush();
    const save = invoke.mock.calls.find(([c]) => c === "ai_save_provider")!;
    expect(save[1]).toMatchObject({ key: "sk-ant-test-123456789", make_default: false, config: { id: "anthropic", kind: "anthropic", model: "claude-opus-5-5" } });
    expect(invoke).toHaveBeenCalledWith("ai_test_provider", { id: "anthropic" });
    expect(screen.getByText('ai.providers.test_ok{"model":"claude-opus-5-5"}')).toBeTruthy();
    fireEvent.click(screen.getByText("btn.close"));
    await expect(p).resolves.toBeUndefined();
  });
});
