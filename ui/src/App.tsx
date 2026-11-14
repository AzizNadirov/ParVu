// The main window. Owns per-window UI state and turns user actions into
// backend commands. All data work happens in Rust; this file only decides
// what to call and what to show.
import { Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import {
  api,
  errorMessage,
  type ApiError,
  type Bootstrap,
  type Cell,
  type ColumnMeta,
  type Operation,
  type Page,
  type Settings,
  type TabSummary,
} from "./lib/api";
import { makeTranslator } from "./lib/i18n";
import { applyThemeCss } from "./lib/theme";
import { UiContext, type Closer, type MessageButton, type Ui } from "./lib/ui";
import { formatRows, pythonTuple } from "./lib/copy";
import type { ExprSchema } from "./lib/exprComplete";
import { Dialog } from "./components/Dialog";
import { MenuBar, PopupMenu, type MenuEntry } from "./components/PopupMenu";
import { DataGrid, isNumericType, type GridHandle } from "./components/DataGrid";
import { QueryEditor, type EditorMode } from "./components/QueryEditor";
import { Pagination, StepsPanel, TabBar, Welcome, stepText } from "./components/Panels";
import type { FormatRule } from "./lib/format";
import {
  SearchPanel,
  openAboutDialog,
  openAppendDialog,
  openBin,
  openChart,
  openColumnStats,
  openColumns,
  openConditionalFormat,
  openConfirmClose,
  openCopyTuple,
  openCrashReport,
  openDropDuplicates,
  openDropNull,
  openExportDialog,
  openExpressionHelp,
  openFillNull,
  openFilter,
  openGroupBy,
  openJoinDialog,
  openLanguageSelector,
  openMathDialog,
  openMelt,
  openPivot,
  openReplace,
  openSettingsDialog,
  openShortcutsDialog,
  openSlice,
  openSort,
  openSplitColumn,
  openTableInfo,
  openThemeSelector,
  openTranspose,
  openUniqueValues,
  openUnsavedChanges,
  openValueCounts,
  openWindow,
  preloadDialogs,
} from "./dialogs/lazy";

/** Per-tab UI state that lives only in the window. */
interface TabUi {
  query: string;
  mode: EditorMode;
  page: number;
  sort: { column: string; ascending: boolean } | null;
  /** Columns frozen on the left. */
  frozen: number;
  /** Column widths set by dragging, by name. */
  widths: Record<string, number>;
  formats: FormatRule[];
  /** Min and max of the columns that have a colour scale. */
  ranges: Record<string, { min: number; max: number }>;
}

const newTabUi = (name: string): TabUi => ({
  query: `SELECT * FROM ${name}`,
  mode: "sql",
  page: 1,
  sort: null,
  frozen: 0,
  widths: {},
  formats: [],
  ranges: {},
});

const NO_WIDTHS: Record<string, number> = {};
const NO_FORMATS: FormatRule[] = [];
const NO_RANGES: Record<string, { min: number; max: number }> = {};

const CHANGE_TYPES = ["INTEGER", "BIGINT", "DOUBLE", "VARCHAR", "BOOLEAN", "DATE", "TIMESTAMP"];

function num(v: unknown, fallback: number): number {
  const n = Number(v);
  return Number.isFinite(n) && n > 0 ? n : fallback;
}

/** `$(name)` substitution as Settings.render_vars does, with the tab's name as the table. */
export function renderVars(template: string, settings: Settings, table: string): string {
  return template
    .split("$(default_data_var_name)").join(table)
    .split("$(default_limit)").join(String(settings.default_limit))
    .split("$(default_sql_font_size)").join(String(settings.default_sql_font_size))
    .split("$(default_sql_font)").join(String(settings.default_sql_font));
}

export function App({ boot }: { boot: Bootstrap }) {
  const [settings, setSettings] = useState<Settings>(boot.settings);
  const [translations, setTranslations] = useState(boot.translations);
  const t = useMemo(() => makeTranslator(translations, boot.english), [translations, boot.english]);
  const [themeKey, setThemeKey] = useState(boot.theme.name);
  const [recents, setRecents] = useState(boot.recents);

  const [tabs, setTabs] = useState<TabSummary[]>([]);
  const [active, setActive] = useState<number | null>(null);
  const tabUi = useRef(new Map<number, TabUi>());
  // Bumped when view-only tab state (widths, frozen columns, formats) changes.
  const [, setViewTick] = useState(0);
  const updateView = useCallback((id: number, patch: Partial<TabUi>) => {
    const st = tabUi.current.get(id);
    if (!st) return;
    Object.assign(st, patch);
    setViewTick((n) => n + 1);
  }, []);
  const [editorText, setEditorText] = useState("");
  const [mode, setMode] = useState<EditorMode>("sql");
  const [page, setPage] = useState<Page | null>(null);
  const [busy, setBusy] = useState(false);
  const [statusText, setStatusText] = useState(t("status.ready"));
  const statusTimer = useRef<number | undefined>(undefined);
  const [dialogs, setDialogs] = useState<{ id: number; node: ReactNode }[]>([]);
  const dialogSeq = useRef(0);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuEntry[] } | null>(null);
  const [editorOpen, setEditorOpen] = useState(true);
  const [stepsOpen, setStepsOpen] = useState(true);
  const [editorHeight, setEditorHeight] = useState(220);
  const [schema, setSchema] = useState<ExprSchema>({ tables: [], functions: boot.functions, methods: [] });
  const [preview, setPreview] = useState("");
  const [search, setSearch] = useState<{ query: string; anchor: { row: number | null; col: number | null } } | null>(null);
  const searchKeys = useRef<{ next(): void; prev(): void } | null>(null);
  const grid = useRef<GridHandle>(null);
  const pendingSelect = useRef<{ row: number; column: string } | null>(null);
  const [gridSelect, setGridSelect] = useState<{ row: number; col: number; seq: number } | null>(null);
  const selectSeq = useRef(0);
  const pageRequest = useRef(0);

  const activeTab = tabs.find((x) => x.id === active) ?? null;
  const allMethods = useMemo(() => [...new Set(Object.values(boot.methods).flat())].sort(), [boot.methods]);

  // ------------------------------------------------------------ ui helpers
  const status = useCallback((text: string, ms?: number) => {
    window.clearTimeout(statusTimer.current);
    setStatusText(text);
    if (ms) statusTimer.current = window.setTimeout(() => setStatusText(""), ms);
  }, []);

  const showDialog = useCallback(<T,>(render: (close: Closer<T>) => ReactNode, _dismiss: T): Promise<T> => {
    return new Promise<T>((resolve) => {
      const id = ++dialogSeq.current;
      const close: Closer<T> = (value) => {
        setDialogs((d) => d.filter((x) => x.id !== id));
        resolve(value);
      };
      setDialogs((d) => [...d, { id, node: render(close) }]);
    });
  }, []);

  const ask = useCallback(
    (title: string, message: string, buttons: MessageButton[]) =>
      showDialog<string>(
        (close) => (
          <Dialog
            title={title}
            onClose={() => close("cancel")}
            footer={buttons.map((b) => (
              <button
                key={b.id}
                className={b.primary ? "primary" : b.danger ? "danger" : ""}
                autoFocus={b.primary}
                onClick={() => close(b.id)}
              >
                {b.label}
              </button>
            ))}
          >
            <div className="message-text">{message}</div>
          </Dialog>
        ),
        "cancel",
      ),
    [showDialog],
  );

  const ui: Ui = useMemo(
    () => ({
      boot,
      t,
      settings,
      setSettings,
      showDialog,
      ask,
      status,
      alert: async (title, message, kind = "info") => {
        const icon = kind === "error" ? "⛔ " : kind === "warning" ? "⚠️ " : "";
        await ask(icon + title, message, [{ id: "ok", label: "OK", primary: true }]);
      },
      confirm: async (title, message) =>
        (await ask(title, message, [
          { id: "cancel", label: t("btn.cancel") },
          { id: "ok", label: "OK", primary: true },
        ])) === "ok",
      prompt: (title, label, initial = "") =>
        showDialog<string | null>((close) => {
          let value = initial;
          return (
            <Dialog
              title={title}
              onClose={() => close(null)}
              onSubmit={() => close(value)}
              footer={
                <>
                  <button onClick={() => close(null)}>{t("btn.cancel")}</button>
                  <button className="primary" onClick={() => close(value)}>
                    OK
                  </button>
                </>
              }
            >
              <label className="col">
                <span>{label}</span>
                <input type="text" autoFocus defaultValue={initial} onChange={(e) => (value = e.target.value)} />
              </label>
            </Dialog>
          );
        }, null),
    }),
    [boot, t, settings, showDialog, ask, status],
  );

  const showError = useCallback(
    (title: string, e: unknown) => {
      const err = e as ApiError;
      const msg =
        err && err.line !== undefined
          ? t("error.expression_at", { line: err.line, column: err.column ?? 0, error: err.message })
          : errorMessage(e);
      return ui.alert(title, msg, "error");
    },
    [t, ui],
  );

  // ------------------------------------------------------------ data loading
  const refreshSchema = useCallback(async () => {
    try {
      const cat = await api.catalog();
      setSchema({
        tables: cat.map(([name, cols]) => ({ name, columns: cols.map((c) => c.name) })),
        functions: boot.functions,
        methods: allMethods,
      });
    } catch {
      /* autocomplete is best-effort */
    }
  }, [boot.functions, allMethods]);

  const loadPage = useCallback(
    async (id: number, p: number) => {
      const req = ++pageRequest.current;
      setBusy(true);
      status(t("status.loading"));
      try {
        const data = await api.getPage(id, p);
        if (req !== pageRequest.current) return;
        setPage(data);
        const st = tabUi.current.get(id);
        if (st) st.page = data.page;
        status(t("status.page_info", { page: data.page, total_pages: data.total_pages }));
        const sel = pendingSelect.current;
        if (sel) {
          pendingSelect.current = null;
          const col = data.columns.findIndex((c) => c.name === sel.column);
          const row = sel.row - data.offset;
          if (col >= 0 && row >= 0 && row < data.rows.length) {
            setGridSelect({ row, col, seq: ++selectSeq.current });
            status(t("search.statusbar.jumped", { row: sel.row + 1, column: sel.column }));
          }
        }
      } catch (e) {
        if (req === pageRequest.current) {
          setPage(null);
          await showError(t("error.query_error"), e);
        }
      } finally {
        if (req === pageRequest.current) setBusy(false);
      }
    },
    [status, t, showError],
  );

  const upsertTab = useCallback((s: TabSummary) => {
    setTabs((list) => (list.some((x) => x.id === s.id) ? list.map((x) => (x.id === s.id ? s : x)) : [...list, s]));
  }, []);

  /** Run a backend call that changes a tab, then show page 1. */
  const mutate = useCallback(
    async (id: number, run: () => Promise<TabSummary>, errorTitle: string, after?: (s: TabSummary) => void) => {
      setBusy(true);
      try {
        const s = await run();
        upsertTab(s);
        after?.(s);
        const last = s.steps[s.steps.length - 1];
        await loadPage(id, 1);
        if (last) status(t("steps.applied", { step: stepText(t, last) }), 4000);
        void refreshSchema();
        return s;
      } catch (e) {
        setBusy(false);
        await showError(errorTitle, e);
        return null;
      }
    },
    [upsertTab, loadPage, status, t, showError, refreshSchema],
  );

  const switchTo = useCallback(
    (id: number) => {
      if (active !== null) {
        const cur = tabUi.current.get(active);
        if (cur) {
          cur.query = editorText;
          cur.mode = mode;
        }
      }
      const st = tabUi.current.get(id);
      setActive(id);
      setEditorText(st?.query ?? "");
      setMode(st?.mode ?? "sql");
      setPreview("");
      void loadPage(id, st?.page ?? 1);
    },
    [active, editorText, mode, loadPage],
  );

  const openPath = useCallback(
    async (path: string) => {
      setBusy(true);
      status(t("status.loading"));
      try {
        const s = await api.openFile(path);
        tabUi.current.set(s.id, newTabUi(s.name));
        upsertTab(s);
        switchTo(s.id);
        void refreshSchema();
        status(t("status.loaded", { filename: path.split(/[\\/]/).pop() ?? path, rows: s.total_rows }), 5000);
      } catch (e) {
        setBusy(false);
        const err = e as ApiError;
        if (err?.kind === "file_not_found") {
          const remove = await ui.confirm(t("error.file_not_found"), t("warning.file_not_found_recent", { path }));
          if (remove) {
            /* recents are cleaned when the list is cleared; keep behaviour simple */
          }
        } else {
          await ui.alert(t("error.load_error"), t("error.load_error_msg", { error: errorMessage(e) }), "error");
        }
      }
    },
    [status, t, upsertTab, switchTo, refreshSchema, ui],
  );

  const browse = useCallback(async () => {
    const exts = boot.extensions.map((e) => e.slice(1));
    const picked = await openDialog({
      multiple: true,
      title: t("dialog.open_file"),
      filters: [
        { name: "Data files", extensions: exts },
        { name: "All files", extensions: ["*"] },
      ],
    });
    const list = Array.isArray(picked) ? picked : picked ? [picked] : [];
    for (const p of list) await openPath(p);
  }, [boot.extensions, openPath, t]);

  // ------------------------------------------------------------ actions
  const withTab = <T,>(fn: (tab: TabSummary) => T): T | undefined => (activeTab ? fn(activeTab) : undefined);

  const execute = useCallback(async () => {
    if (!activeTab) return;
    const text = editorText.trim();
    if (!text) {
      await ui.alert(t("error.empty_query"), t("error.empty_query_msg"), "warning");
      return;
    }
    const id = activeTab.id;
    const st = tabUi.current.get(id);
    if (st) st.sort = null;
    await mutate(
      id,
      () => (mode === "sql" ? api.runSql(id, text) : api.runExpression(id, text)),
      mode === "sql" ? t("error.query_error") : t("error.expression"),
    );
  }, [activeTab, editorText, mode, mutate, t, ui]);

  const applyOp = useCallback(
    async (op: Operation) => {
      if (!activeTab) return;
      return mutate(activeTab.id, () => api.applyOp(activeTab.id, op), t("error.operation_failed"));
    },
    [activeTab, mutate, t],
  );

  const undo = useCallback(async () => {
    if (!activeTab || !activeTab.steps.length) {
      status(t("status.no_transforms"), 3000);
      return;
    }
    const id = activeTab.id;
    setBusy(true);
    try {
      const r = await api.undo(id);
      upsertTab(r.summary);
      const st = tabUi.current.get(id);
      if (st && r.label.key === "step.sort") st.sort = null;
      await loadPage(id, r.label.key === "step.cell_edit" ? (st?.page ?? 1) : 1);
      status(t("steps.undone", { step: stepText(t, r.label) }), 4000);
      void refreshSchema();
    } catch (e) {
      setBusy(false);
      await showError(t("error.undo_failed"), e);
    }
  }, [activeTab, upsertTab, loadPage, status, t, showError, refreshSchema]);

  const resetTab = useCallback(async () => {
    if (!activeTab) return;
    const id = activeTab.id;
    const s = await mutate(id, () => api.resetTab(id), t("error.query_error"));
    if (s) {
      const q = renderVars(settings.default_sql_query, settings, s.name);
      const st = tabUi.current.get(id);
      if (st) {
        st.query = q;
        st.sort = null;
      }
      setEditorText(q);
      status(t("status.reset"), 3000);
    }
  }, [activeTab, mutate, t, settings, status]);

  const sortBy = useCallback(
    async (column: string, ascending?: boolean) => {
      if (!activeTab) return;
      const st = tabUi.current.get(activeTab.id);
      const asc = ascending ?? !(st?.sort?.column === column && st.sort.ascending);
      const s = await applyOp({ op: "sort", column, ascending: asc });
      if (s && st) st.sort = { column, ascending: asc };
    },
    [activeTab, applyOp],
  );

  const confirmLargeDataset = useCallback(async (): Promise<boolean> => {
    if (!activeTab || !settings.enable_large_dataset_warning) return true;
    const rows = activeTab.total_rows;
    let message = "";
    if (settings.warning_criteria === "rows" && rows > settings.warning_threshold_rows) {
      message = t("warning.large_dataset_rows", { rows, threshold: settings.warning_threshold_rows });
    } else if (settings.warning_criteria === "cells") {
      const cols = page?.columns.length ?? 0;
      const cells = rows * cols;
      if (cells > settings.warning_threshold_cells)
        message = t("warning.large_dataset_cells", { cells, rows, columns: cols, threshold: settings.warning_threshold_cells });
    } else if (settings.warning_criteria === "filesize") {
      const mb = activeTab.file_size / (1024 * 1024);
      if (mb > settings.warning_threshold_filesize_mb)
        message = t("warning.large_dataset_size", { size: mb, threshold: settings.warning_threshold_filesize_mb });
    }
    return message ? ui.confirm(t("warning.large_dataset"), message) : true;
  }, [activeTab, settings, page, t, ui]);

  const showUniqueValues = useCallback(
    async (column: string) => {
      if (!activeTab || !(await confirmLargeDataset())) return;
      status(t("status.calculating"));
      try {
        const values = await api.uniqueValues(activeTab.id, column);
        status("");
        if (!values.length) {
          await ui.alert(t("error.no_values"), t("error.no_values_msg", { column }));
          return;
        }
        const chosen = await openUniqueValues(ui, { column, values });
        if (chosen && chosen.length) await applyOp({ op: "filter_values", column, values: chosen });
      } catch (e) {
        status("");
        await showError(t("error.unique_values_error"), e);
      }
    },
    [activeTab, confirmLargeDataset, status, t, ui, applyOp, showError],
  );

  const showStats = useCallback(
    async (column: string) => {
      if (!activeTab) return;
      status(t("column_stats.computing"));
      try {
        const stats = await api.columnStats(activeTab.id, column);
        status("");
        await openColumnStats(ui, { stats });
      } catch (e) {
        status("");
        await ui.alert(t("column_stats.error_title"), t("column_stats.error_msg", { column, error: errorMessage(e) }), "error");
      }
    },
    [activeTab, status, t, ui],
  );

  const copyText = useCallback(
    async (text: string, message: string) => {
      await writeText(text);
      status(message, 3000);
    },
    [status],
  );

  const copyTuple = useCallback(
    async (column: string) => {
      if (!activeTab || !page) return;
      const ci = page.columns.findIndex((c) => c.name === column);
      const pageValues = page.rows.map((r) => r[ci]);
      const numeric = isNumericType(page.columns[ci]?.dtype ?? "");
      if (activeTab.total_rows <= activeTab.page_size) {
        await copyText(pythonTuple(pageValues, numeric), t("status.copied", { count: pageValues.length }));
        return;
      }
      const r = await openCopyTuple(ui, {
        column,
        totalRows: activeTab.total_rows,
        pageSize: activeTab.page_size,
        defaultSample: num(settings.copy_tuple_sample_size, 500),
      });
      if (!r) return;
      try {
        let text = r.mode === "page" ? pythonTuple(pageValues, numeric) : await api.columnTuple(activeTab.id, column, r.mode, r.n);
        if (r.label) text = `${r.label} = ${text}`;
        await copyText(text, t("status.copied", { count: r.mode === "page" ? pageValues.length : r.n }));
      } catch (e) {
        await showError(t("error.operation_failed"), e);
      }
    },
    [activeTab, page, settings, ui, copyText, t, showError],
  );

  const columnNames = page?.columns.map((c) => c.name) ?? [];

  const headerMenu = (column: string): MenuEntry[] => [
    { label: t("context.copy_column"), onSelect: () => void copyText(column, t("status.copied_name")) },
    { type: "separator" },
    {
      label: t("context.change_type"),
      submenu: [
        ...CHANGE_TYPES.map((ty) => ({ label: ty, onSelect: () => void applyOp({ op: "change_type", column, new_type: ty }) })),
        { type: "separator" as const },
        {
          label: t("context.change_type.custom"),
          onSelect: async () => {
            const ty = await ui.prompt(t("context.change_type"), t("context.change_type.prompt"));
            if (ty?.trim()) await applyOp({ op: "change_type", column, new_type: ty.trim() });
          },
        },
      ],
    },
    {
      label: t("context.rename_column"),
      onSelect: async () => {
        const name = await ui.prompt(t("context.rename_dialog.title"), t("context.rename_dialog.prompt"), column);
        if (name?.trim() && name.trim() !== column) await applyOp({ op: "rename", column, new_name: name.trim() });
      },
    },
    { label: t("context.duplicate_column"), onSelect: () => void applyOp({ op: "duplicate", column }) },
    { label: t("context.remove_column"), disabled: columnNames.length < 2, onSelect: () => void applyOp({ op: "remove", column }) },
    { type: "separator" },
    { label: t("context.sort_asc"), onSelect: () => void sortBy(column, true) },
    { label: t("context.sort_desc"), onSelect: () => void sortBy(column, false) },
    { label: t("context.sort_more"), onSelect: () => void runOpDialog(openSort, column) },
    { type: "separator" },
    { label: t("context.filter"), onSelect: () => void runOpDialog(openFilter, column) },
    { label: t("context.copy_values"), onSelect: () => void copyTuple(column) },
    { label: t("context.unique_values"), onSelect: () => void showUniqueValues(column) },
    { label: t("context.value_counts"), onSelect: () => void runOpDialog(openValueCounts, column, true) },
    { label: t("context.column_stats"), onSelect: () => void showStats(column) },
    { label: t("context.chart"), onSelect: () => void showChart(column) },
    { type: "separator" },
    { label: t("context.replace_values"), onSelect: () => void runReplace(column) },
    { label: t("context.fill_null"), onSelect: () => void runOpDialog(openFillNull, column) },
    { label: t("context.drop_null"), onSelect: () => void runDropNull(column) },
    {
      label: t("context.transform"),
      submenu: [
        { label: t("menu.reshape.group_by"), onSelect: () => void runOpDialog(openGroupBy, column, true) },
        { label: t("menu.reshape.pivot"), onSelect: () => void runOpDialog(openPivot, column, true) },
        { label: t("menu.operations.window"), onSelect: () => void runOpDialog(openWindow, column, true) },
        { label: t("menu.data.split"), onSelect: () => void runOpDialog(openSplitColumn, column) },
        { label: t("menu.data.bin"), onSelect: () => void runOpDialog(openBin, column) },
      ],
    },
    { type: "separator" },
    {
      label: t("context.freeze_here"),
      onSelect: () => activeTab && updateView(activeTab.id, { frozen: columnNames.indexOf(column) + 1 }),
    },
    {
      label: t("context.unfreeze"),
      disabled: !(activeTab && (tabUi.current.get(activeTab.id)?.frozen ?? 0) > 0),
      onSelect: () => activeTab && updateView(activeTab.id, { frozen: 0 }),
    },
    { label: t("context.conditional_format"), onSelect: () => void editFormats(column) },
    {
      label: t("context.clear_format"),
      disabled: !(activeTab && tabUi.current.get(activeTab.id)?.formats.some((f) => f.column === column)),
      onSelect: () => {
        const st = activeTab && tabUi.current.get(activeTab.id);
        if (st && activeTab) updateView(activeTab.id, { formats: st.formats.filter((f) => f.column !== column) });
      },
    },
  ];

  const cellMenu = (row: number, col: number): MenuEntry[] => {
    const sel = grid.current?.selection();
    const copy = (format: "tsv" | "csv" | "markdown", headers = false) => {
      if (!sel) return;
      void copyText(
        formatRows(sel.rows, format, headers || format === "markdown" ? sel.columns : undefined),
        t("status.copied_cells", { rows: sel.rows.length, cols: sel.columns.length }),
      );
    };
    const value = page?.rows[row]?.[col] ?? null;
    const column = page?.columns[col]?.name ?? "";
    return [
      { label: t("context.copy"), shortcut: "Ctrl+C", onSelect: () => copy("tsv") },
      { label: t("context.copy_csv"), onSelect: () => copy("csv") },
      { label: t("context.copy_markdown"), onSelect: () => copy("markdown") },
      { label: t("context.copy_with_headers"), onSelect: () => copy("tsv", true) },
      { type: "separator" },
      { label: t("context.filter_by_value"), onSelect: () => void applyOp({ op: "filter_values", column, values: [value] }) },
      { label: t("context.filter_custom"), onSelect: () => void runFilterFromCell(column, value) },
      {
        label: t("context.fill_down"),
        shortcut: "Ctrl+D",
        disabled: !page?.editable || (grid.current?.selectionRange()?.height ?? 0) < 2,
        onSelect: () => void fillDown(),
      },
      {
        label: t("context.set_null"),
        disabled: !page?.editable || value === null,
        onSelect: () => void editCell(row, column, null),
      },
    ];
  };

  const editCell = useCallback(
    async (row: number, column: string, value: Cell) => {
      if (!activeTab || !page) return;
      if (!page.editable) {
        status(t("status.read_only"), 4000);
        return;
      }
      try {
        const s = await api.editCell(activeTab.id, page.offset + row, column, value);
        upsertTab(s);
        await loadPage(activeTab.id, page.page);
      } catch (e) {
        await loadPage(activeTab.id, page.page);
        status(t("status.edit_rejected", { error: errorMessage(e) }), 8000);
      }
    },
    [activeTab, page, upsertTab, loadPage, status, t],
  );

  const runReplace = useCallback(
    async (selected?: string) => {
      if (!activeTab) return;
      const req = await openReplace(ui, { table: activeTab.name, columns: columnNames, selected });
      if (req) await mutate(activeTab.id, () => api.replaceValues(activeTab.id, req), t("error.operation_failed"));
    },
    [activeTab, ui, columnNames, mutate, t],
  );

  const runDropNull = useCallback(
    async (selected?: string) => {
      if (!activeTab) return;
      const op = await openDropNull(ui, { table: activeTab.name, columns: columnNames, selected });
      if (op) await applyOp(op);
    },
    [activeTab, ui, columnNames, applyOp],
  );

  const otherTables = async () => {
    const cat = await api.catalog();
    return cat.filter(([n]) => n !== activeTab?.name).map(([name, cols]) => ({ name, columns: cols.map((c) => c.name) }));
  };

  const runJoin = async () =>
    withTab(async (tab) => {
      const others = await otherTables();
      if (!others.length) return ui.alert(t("menu.operations.join"), t("error.join_needs_tab"));
      const op = await openJoinDialog(ui, { table: tab.name, columns: columnNames, others });
      if (op) await applyOp(op);
    });

  const runAppend = async () =>
    withTab(async (tab) => {
      const others = await otherTables();
      if (!others.length) return ui.alert(t("menu.operations.append"), t("error.append_needs_tab"));
      const op = await openAppendDialog(ui, { table: tab.name, others: others.map((o) => o.name) });
      if (op) await applyOp(op);
    });

  const runDropDuplicates = async () =>
    withTab(async (tab) => {
      const op = await openDropDuplicates(ui, { table: tab.name, columns: columnNames });
      if (op) await applyOp(op);
    });

  const runMath = async () =>
    withTab(async (tab) => {
      const r = await openMathDialog(ui, { tabId: tab.id, table: tab.name, columns: columnNames });
      if (r) await mutate(tab.id, () => api.addComputedColumn(tab.id, r.name, r.text), t("error.operation_failed"));
    });

  /** Open an operation dialog for the active tab and apply its result. */
  type OpDialog = (
    ui: Ui,
    a: { table: string; columns: ColumnMeta[]; selected?: string },
  ) => Promise<Operation | null>;
  const runOpDialog = async (open: OpDialog, selected?: string, expensive = false) =>
    withTab(async (tab) => {
      if (expensive && !(await confirmLargeDataset())) return;
      const op = await open(ui, { table: tab.name, columns: page?.columns ?? [], selected });
      if (!op) return;
      const s = await applyOp(op);
      // A multi-column sort replaces the header sort indicator.
      const st = tabUi.current.get(tab.id);
      if (s && st && op.op === "sort_by") st.sort = null;
    });

  const runTranspose = async () =>
    withTab(async (tab) => {
      const op = await openTranspose(ui, { table: tab.name, columns: page?.columns ?? [], rows: tab.total_rows });
      if (op) await applyOp(op);
    });

  const runSlice = async () =>
    withTab(async (tab) => {
      const op = await openSlice(ui, { table: tab.name, rows: tab.total_rows });
      if (op) await applyOp(op);
    });

  const runDescribe = async () =>
    withTab(async () => {
      if (await confirmLargeDataset()) await applyOp({ op: "describe" });
    });

  const runFilterFromCell = async (column: string, value: Cell) =>
    withTab(async (tab) => {
      const op = await openFilter(ui, {
        table: tab.name,
        columns: page?.columns ?? [],
        selected: column,
        initial: value === null ? { op: "is_null" } : { op: "eq", value },
      });
      if (op) await applyOp(op);
    });

  /** Copy the first row of the selection into the selected rows below it (Ctrl+D). */
  const fillDown = async () =>
    withTab(async (tab) => {
      const range = grid.current?.selectionRange();
      if (!page || !range || range.height < 2) {
        status(t("status.fill_down_select"), 4000);
        return;
      }
      if (!page.editable) {
        status(t("status.read_only"), 4000);
        return;
      }
      const cells = [];
      for (let c = range.x; c < range.x + range.width; c++) {
        const column = page.columns[c]?.name;
        if (!column) continue;
        const value = page.rows[range.y]?.[c] ?? null;
        for (let r = range.y + 1; r < range.y + range.height; r++) cells.push({ row: page.offset + r, column, value });
      }
      try {
        const s = await api.editCells(tab.id, cells);
        upsertTab(s);
        await loadPage(tab.id, page.page);
        const last = s.steps[s.steps.length - 1];
        if (last) status(t("steps.applied", { step: stepText(t, last) }), 4000);
      } catch (e) {
        await loadPage(tab.id, page.page);
        status(t("status.edit_rejected", { error: errorMessage(e) }), 8000);
      }
    });

  const showChart = async (selected?: string) =>
    withTab(async (tab) => {
      if (!(await confirmLargeDataset())) return;
      await openChart(ui, { tabId: tab.id, table: tab.name, columns: page?.columns ?? [], selected });
    });

  const editFormats = async (selected?: string) =>
    withTab(async (tab) => {
      const st = tabUi.current.get(tab.id);
      if (!st) return;
      const rules = await openConditionalFormat(ui, { columns: page?.columns ?? [], rules: st.formats, selected });
      if (rules) updateView(tab.id, { formats: rules });
    });

  const showTableInfo = async () =>
    withTab(async (tab) => openTableInfo(ui, { summary: tab, columns: await api.columns(tab.id) }));

  const exportTab = useCallback(async (): Promise<boolean> => {
    if (!activeTab) {
      await ui.alert(t("warning.no_data"), t("warning.no_data_msg"), "warning");
      return false;
    }
    const columns = await api.columns(activeTab.id);
    const defaultDir = activeTab.path.replace(/[\\/][^\\/]*$/, "");
    const done = await openExportDialog(ui, { tabId: activeTab.id, tableName: activeTab.name, columns, defaultDir });
    if (!done) return false;
    if (done.ok) {
      status(t("status.exported", { rows: done.rows, path: done.path }), 6000);
      return true;
    }
    if (done.cancelled) status(t("status.export_cancelled"), 4000);
    else if (done.error) await ui.alert(t("error.export_error"), t("error.export_error_msg", { error: done.error }), "error");
    return false;
  }, [activeTab, ui, t, status]);

  const saveTab = useCallback(
    async (tab: TabSummary, path: string | null): Promise<boolean> => {
      setBusy(true);
      status(t("status.saving"));
      try {
        const s = await api.saveTab(tab.id, path);
        upsertTab(s);
        const st = tabUi.current.get(tab.id);
        if (st) st.sort = null;
        if (tab.id === active) await loadPage(tab.id, 1);
        else setBusy(false);
        void refreshSchema();
        await ui.alert(t("success.save_complete"), t("success.save_complete_msg", { path: s.path }));
        return true;
      } catch (e) {
        setBusy(false);
        await ui.alert(t("error.save_failed"), t("error.save_failed_msg", { error: errorMessage(e) }), "error");
        return false;
      }
    },
    [status, t, upsertTab, active, loadPage, refreshSchema, ui],
  );

  const saveAs = useCallback(
    async (tab: TabSummary) => {
      const path = await saveDialog({
        title: t("dialog.save_as"),
        defaultPath: tab.path,
        filters: [
          { name: "Parquet", extensions: ["parquet"] },
          { name: "CSV", extensions: ["csv", "tsv"] },
          { name: "JSON", extensions: ["json", "jsonl"] },
          { name: "Excel", extensions: ["xlsx"] },
        ],
      });
      return path ? saveTab(tab, path) : false;
    },
    [saveTab, t],
  );

  const save = useCallback(async () => {
    if (!activeTab) return;
    if (!activeTab.steps.length && !activeTab.pending_edits) return;
    await saveTab(activeTab, null);
  }, [activeTab, saveTab]);

  /** Ask about unsaved edits of `tab`. True when it may be closed. */
  const resolveUnsaved = useCallback(
    async (tab: TabSummary): Promise<boolean> => {
      if (!tab.pending_edits) return true;
      if (tab.id !== active) switchTo(tab.id);
      const choice = await openUnsavedChanges(ui, { count: tab.pending_edits });
      if (choice === "cancel") return false;
      if (choice === "save") return saveTab(tab, null);
      return true;
    },
    [active, switchTo, ui, saveTab],
  );

  const closeTab = useCallback(
    async (id: number) => {
      const tab = tabs.find((x) => x.id === id);
      if (!tab || !(await resolveUnsaved(tab))) return;
      try {
        await api.closeTab(id);
      } catch {
        /* already gone */
      }
      tabUi.current.delete(id);
      const rest = tabs.filter((x) => x.id !== id);
      setTabs(rest);
      if (id === active) {
        if (rest.length) {
          const idx = Math.min(tabs.findIndex((x) => x.id === id), rest.length - 1);
          switchTo(rest[idx].id);
        } else {
          setActive(null);
          setPage(null);
          setEditorText("");
          status(t("status.ready"));
        }
      }
      void refreshSchema();
    },
    [tabs, active, resolveUnsaved, switchTo, status, t, refreshSchema],
  );

  const openSearch = useCallback(() => {
    if (!activeTab) {
      void ui.alert(t("search.menu_msgbox.title"), t("search.menu_msgbox.no_file"));
      return;
    }
    const cur = grid.current?.currentCell();
    const sel = grid.current?.selection();
    const preset = sel?.rows[0]?.[0] ?? "";
    setSearch({
      query: preset && preset.length < 80 ? preset : search?.query ?? "",
      anchor: { row: cur && page ? page.offset + cur.row : null, col: cur?.col ?? null },
    });
  }, [activeTab, ui, t, page, search]);

  const jumpTo = useCallback(
    (row: number, column: string) => {
      if (!activeTab || !page) return;
      const target = Math.floor(row / activeTab.page_size) + 1;
      if (target !== page.page) {
        pendingSelect.current = { row, column };
        void loadPage(activeTab.id, target);
      } else {
        const col = page.columns.findIndex((c) => c.name === column);
        if (col >= 0) grid.current?.selectCell(row - page.offset, col);
        status(t("search.statusbar.jumped", { row: row + 1, column }));
      }
    },
    [activeTab, page, loadPage, status, t],
  );

  // ------------------------------------------------------------ settings, theme, language
  const onTheme = useCallback((info: { name: string; css: string }) => {
    applyThemeCss(info.css);
    setThemeKey(info.name + Date.now());
  }, []);
  const onLanguage = useCallback((_lang: string, tr: Record<string, string>) => setTranslations(tr), []);

  // ------------------------------------------------------------ menus
  const menus = [
    {
      title: t("menu.file"),
      items: (): MenuEntry[] => [
        { label: t("menu.file.new_window"), shortcut: "Ctrl+N", onSelect: () => void api.newWindow() },
        { type: "separator" },
        { label: t("menu.file.open"), shortcut: "Ctrl+O", onSelect: () => void browse() },
        {
          label: t("menu.file.recent_files"),
          submenu: recents.length
            ? [
                ...recents.map((r) => ({ label: r, onSelect: () => void openPath(r) })),
                { type: "separator" as const },
                { label: t("menu.file.clear_recents"), onSelect: () => void api.clearRecents() },
              ]
            : [{ label: t("menu.file.no_recents"), disabled: true }],
        },
        { type: "separator" },
        { label: t("menu.file.save"), shortcut: "Ctrl+S", disabled: !activeTab, onSelect: () => void save() },
        { label: t("menu.file.save_as"), shortcut: "Ctrl+Shift+S", disabled: !activeTab, onSelect: () => activeTab && void saveAs(activeTab) },
        { label: t("menu.file.export"), shortcut: "Ctrl+E", disabled: !activeTab, onSelect: () => void exportTab() },
        { type: "separator" },
        { label: t("menu.file.close_tab"), shortcut: "Ctrl+W", disabled: !activeTab, onSelect: () => activeTab && void closeTab(activeTab.id) },
        { label: t("menu.file.settings"), onSelect: () => void openSettingsDialog(ui, { onSettings: setSettings, onTheme, onLanguage }) },
        { type: "separator" },
        { label: t("menu.file.exit"), onSelect: () => void getCurrentWindow().close() },
      ],
    },
    {
      title: t("menu.edit"),
      items: (): MenuEntry[] => [
        { label: t("menu.edit.undo"), shortcut: "Ctrl+Z", disabled: !activeTab?.steps.length, onSelect: () => void undo() },
        { label: t("menu.edit.reset"), disabled: !activeTab, onSelect: () => void resetTab() },
        { type: "separator" },
        { label: t("menu.edit.find"), shortcut: "Ctrl+F", disabled: !activeTab, onSelect: openSearch },
        { label: t("menu.edit.find_next"), shortcut: "F3", disabled: !activeTab, onSelect: () => (search ? searchKeys.current?.next() : openSearch()) },
        { label: t("menu.edit.find_prev"), shortcut: "Shift+F3", disabled: !activeTab, onSelect: () => (search ? searchKeys.current?.prev() : openSearch()) },
      ],
    },
    {
      title: t("menu.view"),
      items: (): MenuEntry[] => [
        { label: t("menu.view.editor"), checked: editorOpen, onSelect: () => setEditorOpen((v) => !v) },
        { label: t("menu.view.steps"), checked: stepsOpen, onSelect: () => setStepsOpen((v) => !v) },
        { type: "separator" },
        { label: t("menu.view.table_info"), disabled: !activeTab, onSelect: () => void showTableInfo() },
        { label: t("menu.view.chart"), shortcut: "Ctrl+Shift+C", disabled: !activeTab, onSelect: () => void showChart() },
        { label: t("menu.view.conditional_format"), disabled: !activeTab, onSelect: () => void editFormats() },
        {
          label: t("menu.view.unfreeze"),
          disabled: !(activeTab && (tabUi.current.get(activeTab.id)?.frozen ?? 0) > 0),
          onSelect: () => activeTab && updateView(activeTab.id, { frozen: 0 }),
        },
        { type: "separator" },
        { label: t("menu.view.theme"), onSelect: () => void openThemeSelector(ui, { onTheme }) },
        { label: t("menu.view.language"), onSelect: () => void openLanguageSelector(ui, { onLanguage }) },
      ],
    },
    {
      title: t("menu.data"),
      items: (): MenuEntry[] => [
        { label: t("menu.data.filter"), shortcut: "Ctrl+Shift+L", disabled: !activeTab, onSelect: () => void runOpDialog(openFilter) },
        { label: t("menu.data.sort"), disabled: !activeTab, onSelect: () => void runOpDialog(openSort) },
        { label: t("menu.data.columns"), disabled: !activeTab, onSelect: () => void runOpDialog(openColumns) },
        { label: t("menu.data.slice"), disabled: !activeTab, onSelect: () => void runSlice() },
        { type: "separator" },
        { label: t("menu.data.fill_null"), disabled: !activeTab, onSelect: () => void runOpDialog(openFillNull) },
        { label: t("menu.operations.drop_null"), disabled: !activeTab, onSelect: () => void runDropNull() },
        { label: t("menu.operations.drop_duplicates"), disabled: !activeTab, onSelect: () => void runDropDuplicates() },
        { label: t("menu.operations.replace"), disabled: !activeTab, onSelect: () => void runReplace() },
        { type: "separator" },
        { label: t("menu.data.split"), disabled: !activeTab, onSelect: () => void runOpDialog(openSplitColumn) },
        { label: t("menu.data.bin"), disabled: !activeTab, onSelect: () => void runOpDialog(openBin) },
      ],
    },
    {
      title: t("menu.reshape"),
      items: (): MenuEntry[] => [
        { label: t("menu.reshape.group_by"), shortcut: "Ctrl+G", disabled: !activeTab, onSelect: () => void runOpDialog(openGroupBy, undefined, true) },
        { label: t("menu.reshape.pivot"), disabled: !activeTab, onSelect: () => void runOpDialog(openPivot, undefined, true) },
        { label: t("menu.reshape.melt"), disabled: !activeTab, onSelect: () => void runOpDialog(openMelt) },
        { label: t("menu.reshape.transpose"), disabled: !activeTab, onSelect: () => void runTranspose() },
        { type: "separator" },
        { label: t("menu.reshape.value_counts"), disabled: !activeTab, onSelect: () => void runOpDialog(openValueCounts, undefined, true) },
        { label: t("menu.reshape.describe"), disabled: !activeTab, onSelect: () => void runDescribe() },
      ],
    },
    {
      title: t("menu.operations"),
      items: (): MenuEntry[] => [
        { label: t("menu.operations.math"), disabled: !activeTab, onSelect: () => void runMath() },
        { label: t("menu.operations.window"), disabled: !activeTab, onSelect: () => void runOpDialog(openWindow, undefined, true) },
        { type: "separator" },
        { label: t("menu.operations.join"), disabled: !activeTab, onSelect: () => void runJoin() },
        { label: t("menu.operations.append"), disabled: !activeTab, onSelect: () => void runAppend() },
      ],
    },
    {
      title: t("menu.help"),
      items: (): MenuEntry[] => [
        { label: t("menu.help.expression"), onSelect: () => void openExpressionHelp(ui) },
        { label: t("menu.help.shortcuts"), shortcut: "Ctrl+/", onSelect: () => void openShortcutsDialog(ui) },
        { type: "separator" },
        { label: t("menu.help.about"), onSelect: () => void openAboutDialog(ui) },
      ],
    },
  ];

  // ------------------------------------------------------------ effects
  // Keep refs to the latest handlers for global listeners.
  const filterRows = () => void runOpDialog(openFilter);
  const groupBy = () => void runOpDialog(openGroupBy, undefined, true);
  const handlers = useRef({ browse, save, saveAs, exportTab, undo, openSearch, closeTab, openPath, activeTab, search, dialogs, filterRows, groupBy, fillDown, showChart });
  handlers.current = { browse, save, saveAs, exportTab, undo, openSearch, closeTab, openPath, activeTab, search, dialogs, filterRows, groupBy, fillDown, showChart };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const h = handlers.current;
      if (h.dialogs.length || e.defaultPrevented) return;
      const ctrl = e.ctrlKey || e.metaKey;
      const target = e.target as HTMLElement;
      const typing = target.closest(".cm-editor, input, textarea, select");
      const k = e.key.toLowerCase();
      const run = (fn: () => void) => {
        e.preventDefault();
        fn();
      };
      if (ctrl && !e.shiftKey && k === "o") run(() => void h.browse());
      else if (ctrl && !e.shiftKey && k === "n") run(() => void api.newWindow());
      else if (ctrl && e.shiftKey && k === "s") run(() => h.activeTab && void h.saveAs(h.activeTab));
      else if (ctrl && k === "s") run(() => void h.save());
      else if (ctrl && k === "e") run(() => void h.exportTab());
      else if (ctrl && k === "w") run(() => h.activeTab && void h.closeTab(h.activeTab.id));
      else if (ctrl && e.shiftKey && k === "l") run(() => h.activeTab && h.filterRows());
      else if (ctrl && !e.shiftKey && k === "g") run(() => h.activeTab && h.groupBy());
      else if (ctrl && e.shiftKey && k === "c" && !typing) run(() => h.activeTab && void h.showChart());
      else if (ctrl && !e.shiftKey && k === "d" && !typing) run(() => h.activeTab && void h.fillDown());
      else if (ctrl && k === "f") run(h.openSearch);
      else if (ctrl && (k === "/" || e.code === "Slash")) run(() => void openShortcutsDialog(ui));
      else if (e.key === "F3") run(() => (h.search ? (e.shiftKey ? searchKeys.current?.prev() : searchKeys.current?.next()) : h.openSearch()));
      else if (ctrl && !e.shiftKey && k === "z" && !typing) run(() => void h.undo());
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [ui]);

  useEffect(() => {
    const unsubs: Promise<() => void>[] = [
      listen<string[]>("recents-changed", (e) => setRecents(e.payload)),
      getCurrentWebview().onDragDropEvent(async (e) => {
        if (e.payload.type !== "drop") return;
        for (const p of e.payload.paths) {
          const ext = "." + (p.split(".").pop() ?? "").toLowerCase();
          if (boot.extensions.includes(ext)) await handlers.current.openPath(p);
          else status(t("error.unsupported_drop", { path: p }), 5000);
        }
      }),
    ];
    return () => unsubs.forEach((u) => void u.then((f) => f()));
  }, [boot.extensions, status, t]);

  // Closing the window: unsaved edits, then (optionally) the transforms warning.
  const closeState = useRef({ tabs, settings, resolveUnsaved, exportTab, ui });
  closeState.current = { tabs, settings, resolveUnsaved, exportTab, ui };
  useEffect(() => {
    const win = getCurrentWindow();
    const un = win.onCloseRequested(async (event) => {
      event.preventDefault();
      const c = closeState.current;
      for (const tab of c.tabs) if (!(await c.resolveUnsaved(tab))) return;
      const withSteps = c.tabs.filter((x) => x.steps.length && !x.pending_edits);
      if (c.settings.warn_on_exit_with_transforms && withSteps.length) {
        const r = await openConfirmClose(c.ui, { tabs: withSteps.map((x) => x.name) });
        if (r.result === "cancel") return;
        if (r.result === "save_and_close" && !(await c.exportTab())) return;
        if (r.dontAsk) {
          const s = await api.updateSettings({ warn_on_exit_with_transforms: false });
          setSettings(s);
          await c.ui.alert(t("dialog.confirm_close.setting_saved"), t("dialog.confirm_close.setting_saved_msg"));
        }
      }
      await win.destroy();
    });
    return () => void un.then((f) => f());
  }, [t]);

  // First start: crash report and files from the command line. React runs
  // effects twice in development, so guard against doing this twice.
  const started = useRef(false);
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    window.setTimeout(preloadDialogs, 1500);
    if (boot.crash_report && settings.enable_crash_reporting) void openCrashReport(ui, { report: boot.crash_report });
    void (async () => {
      for (const f of boot.initial_files) await openPath(f);
    })();
    // Runs once per window.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Live SQL preview in expression mode.
  useEffect(() => {
    if (mode !== "expression" || !activeTab) {
      setPreview("");
      return;
    }
    const text = editorText.trim();
    if (!text) {
      setPreview(t("editor.preview_empty"));
      return;
    }
    const id = activeTab.id;
    const timer = window.setTimeout(async () => {
      try {
        const p = await api.previewExpression(id, text);
        setPreview(t("editor.preview", { sql: p.sql }));
      } catch (e) {
        setPreview(t("editor.preview_invalid", { error: errorMessage(e) }));
      }
    }, 250);
    return () => window.clearTimeout(timer);
  }, [editorText, mode, activeTab, t]);

  useEffect(() => {
    void getCurrentWindow().setTitle(activeTab ? `ParVu — ${activeTab.name}` : "ParVu");
  }, [activeTab]);

  // ------------------------------------------------------------ render
  const st = activeTab ? tabUi.current.get(activeTab.id) : undefined;

  // Colour scales need each column's min and max over the whole result;
  // re-read them when the data or the scaled columns change.
  const scaleColumns = [...new Set((st?.formats ?? []).filter((f) => f.kind === "scale").map((f) => f.column))];
  const rangeKey = activeTab ? `${activeTab.id}|${activeTab.current_query}|${activeTab.pending_edits}|${scaleColumns.join("\u0000")}` : "";
  useEffect(() => {
    if (!activeTab || !scaleColumns.length) return;
    const id = activeTab.id;
    let cancelled = false;
    void Promise.all(scaleColumns.map((c) => api.columnRange(id, c).then((r) => [c, r] as const).catch(() => [c, null] as const))).then(
      (pairs) => {
        if (cancelled) return;
        const ranges: Record<string, { min: number; max: number }> = {};
        for (const [c, r] of pairs) if (r) ranges[c] = { min: r[0], max: r[1] };
        updateView(id, { ranges });
      },
    );
    return () => {
      cancelled = true;
    };
    // rangeKey covers the tab, its data and the scaled columns.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rangeKey]);

  const sqlFont = String(settings.default_sql_font || "monospace");
  const sqlSize = num(settings.default_sql_font_size, 11);
  const headerLabel = activeTab
    ? t(mode === "sql" ? "label.sql_query" : "label.expression_query", { table_name: activeTab.name })
    : "";

  const startResize = (e: React.MouseEvent) => {
    e.preventDefault();
    const startY = e.clientY;
    const startH = editorHeight;
    const move = (ev: MouseEvent) => setEditorHeight(Math.max(80, Math.min(window.innerHeight - 250, startH + ev.clientY - startY)));
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  return (
    <UiContext.Provider value={ui}>
      <div className="app">
        <MenuBar menus={menus} />
        {!activeTab ? (
          <Welcome t={t} recents={recents} extensions={boot.extensions} onOpen={(p) => void openPath(p)} onBrowse={() => void browse()} />
        ) : (
          <main className="workspace">
            {editorOpen && (
              <section className="editor-panel" style={{ height: editorHeight }}>
                <div className="editor-toolbar row">
                  <strong className="ellipsis">{headerLabel}</strong>
                  <span className="spacer" />
                  <span className="muted">{t("editor.mode")}</span>
                  <div className="segmented">
                    <button
                      className={mode === "sql" ? "active" : ""}
                      onClick={() => {
                        setMode("sql");
                        if (st) st.mode = "sql";
                      }}
                    >
                      {t("editor.mode.sql")}
                    </button>
                    <button
                      className={mode === "expression" ? "active" : ""}
                      onClick={() => {
                        setMode("expression");
                        if (st) st.mode = "expression";
                      }}
                    >
                      {t("editor.mode.expression")}
                    </button>
                  </div>
                </div>
                <QueryEditor
                  mode={mode}
                  value={editorText}
                  onChange={(v) => {
                    setEditorText(v);
                    if (st) st.query = v;
                  }}
                  onRun={() => void execute()}
                  schema={schema}
                  keywords={settings.sql_keywords ?? []}
                  fontFamily={sqlFont}
                  fontSize={sqlSize}
                />
                {mode === "expression" && <div className="expr-preview mono ellipsis" title={preview}>{preview}</div>}
                <div className="editor-actions row">
                  <button className="primary" disabled={busy} onClick={() => void execute()} title={t("editor.run_hint")}>
                    ▶ {t("btn.execute")}
                  </button>
                  <button disabled={busy} onClick={() => void resetTab()}>
                    {t("btn.reset")}
                  </button>
                  <button onClick={() => void showTableInfo()}>{t("btn.table_info")}</button>
                  <span className="muted small">{t("editor.run_hint")}</span>
                  <span className="spacer" />
                  <button onClick={() => void runOpDialog(openFilter)}>⏷ {t("btn.filter")}</button>
                  <button onClick={() => void runOpDialog(openGroupBy, undefined, true)}>Σ {t("btn.group_by")}</button>
                  <button onClick={() => void showChart()}>📊 {t("btn.chart")}</button>
                  <button onClick={() => void runMath()}>🧮 {t("menu.operations.math")}</button>
                  <button onClick={() => void runJoin()}>🔗 {t("menu.operations.join")}</button>
                  <button onClick={() => void runAppend()}>⬇ {t("menu.operations.append")}</button>
                </div>
                <div className="resize-handle" onMouseDown={startResize} />
              </section>
            )}
            <StepsPanel
              t={t}
              steps={activeTab.steps}
              open={stepsOpen}
              onToggle={() => setStepsOpen((v) => !v)}
              onUndo={() => void undo()}
              busy={busy}
            />
            <section className="grid-panel">
              <DataGrid
                ref={grid}
                page={page}
                sort={st?.sort ?? null}
                themeKey={themeKey}
                nullText={t("grid.null")}
                select={gridSelect}
                onHeaderClick={(c) => void sortBy(c)}
                onHeaderMenu={(c, x, y) => setMenu({ x, y, items: headerMenu(c) })}
                onCellMenu={(r, c, x, y) => setMenu({ x, y, items: cellMenu(r, c) })}
                onEdit={(r, c, v) => void editCell(r, c, v)}
                frozen={st?.frozen ?? 0}
                widths={st?.widths ?? NO_WIDTHS}
                onResize={(c, w) => activeTab && updateView(activeTab.id, { widths: { ...(st?.widths ?? {}), [c]: w } })}
                formats={st?.formats ?? NO_FORMATS}
                ranges={st?.ranges ?? NO_RANGES}
              />
              {busy && <div className="busy-bar" />}
            </section>
            {page && (
              <Pagination
                t={t}
                page={page.page}
                totalPages={page.total_pages}
                totalRows={page.total_rows}
                pageSize={page.page_size}
                busy={busy}
                onPage={(p) => void loadPage(activeTab.id, p)}
              />
            )}
          </main>
        )}
        {tabs.length > 0 && (
          <TabBar t={t} tabs={tabs} active={active} onSelect={switchTo} onClose={(id) => void closeTab(id)} onAdd={() => void browse()} />
        )}
        <footer className="status-bar row">
          <span className="ellipsis grow">{statusText}</span>
          {activeTab && !activeTab.is_base_query && <span className="muted">{t("status.read_only")}</span>}
          {activeTab && activeTab.pending_edits > 0 && (
            <span className="warn-text">📝 {t("status.unsaved_edits", { count: activeTab.pending_edits })}</span>
          )}
          {activeTab && <span className="nowrap">{t("pagination.rows", { total_rows: activeTab.total_rows })}</span>}
        </footer>
      </div>
      {search && activeTab && (
        <Suspense fallback={null}>
        <SearchPanel
          key={activeTab.id}
          tabId={activeTab.id}
          columns={columnNames}
          initialQuery={search.query}
          anchor={search.anchor}
          onJump={jumpTo}
          onClose={() => {
            setSearch(null);
            searchKeys.current = null;
            grid.current?.focus();
          }}
          registerKeys={(k) => (searchKeys.current = k)}
        />
        </Suspense>
      )}
      {menu && <PopupMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />}
      {dialogs.map((d) => (
        <div key={d.id}>{d.node}</div>
      ))}
    </UiContext.Provider>
  );
}
