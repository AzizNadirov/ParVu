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
  type ColumnFn,
  type ColumnMeta,
  type CsvOptions,
  setErrorTranslator,
  isTableOp,
  type Derivation,
  type DatePartName,
  type TimeUnitName,
  type AiRunEvent,
  type AiRunFinished,
  type McpProposal,
  type ProvidersInfo,
  type Operation,
  type Page,
  type Reading,
  type Settings,
  type SourceColumn,
  type SheetsView,
  type TabSummary,
  type CustomDefinition,
  type CustomOffer,
  type FunctionDef,
} from "./lib/api";
import { makeTranslator } from "./lib/i18n";
import { dayFirst, readAsChoices, sameReading, swapDayMonth } from "./lib/types";
import { columnHighlights, errorOffset, formulaRefs, newFormulaName, refFor } from "./lib/formula";
import { openUrl } from "@tauri-apps/plugin-opener";
import { jobResult } from "./lib/jobs";
import { openSetValue } from "./dialogs/SetValueDialog";
import { sheetMenu, sheetsToOpen, type SheetActions } from "./lib/sheets";
import { StatusJobs } from "./components/JobProgress";
import { fillEdge, fillFromHandle, fillSeries, type FillCell, type FillDirection, type Rect } from "./lib/fill";
import { FormulaBar, type FormulaBarHandle } from "./components/FormulaBar";
import { isDark } from "./lib/chart";
import { DATE_FILTERS, EXTRACT_FNS, fnLabelKey, hasTime, isDateFamily, previewValues, TEXT_FNS, type FnName } from "./lib/colfn";
import { applyThemeCss, resolvedColor } from "./lib/theme";
import { UiContext, type Closer, type MessageButton, type Ui } from "./lib/ui";
import { formatRows, pythonTuple } from "./lib/copy";
import type { ExprSchema } from "./lib/exprComplete";
import { Dialog } from "./components/Dialog";
import { MenuBar, PopupMenu, type MenuEntry } from "./components/PopupMenu";
import { DataGrid, isNumericType, type GridHandle } from "./components/DataGrid";
import { QueryEditor, type EditorMode } from "./components/QueryEditor";
import { Pagination, StepsPanel, TabBar, Welcome, stepText } from "./components/Panels";
import { AiPanel, type AiActions } from "./components/AiPanel";
import { applyEvent, emptyChat, finishRun, startRun, type ChatState } from "./lib/ai";
import type { FormatRule } from "./lib/format";
import {
  SearchPanel,
  openAboutDialog,
  openAppendDialog,
  openBin,
  openChart,
  openColumnStats,
  openAiProviders,
  openConnectAi,
  openMcpProposal,
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
  openConvert,
  openColumnFn,
  openMergeColumns,
  openDateDiff,
  openLastN,
  openExpandJson,
  openOpenOptions,
  openTableInfo,
  openThemeSelector,
  openTranspose,
  openUniqueValues,
  openSeries,
  openUnsavedChanges,
  openValueCounts,
  openWindow,
  preloadDialogs,
  openCustomTools,
  openCustomEditor,
  openCustomApply,
  openSaveSteps,
} from "./dialogs/lazy";
import {
  CUSTOM_MENU_MAX,
  customAggregates,
  customDefinitions,
  functionsForColumn,
  isUsable,
  newDefinition,
  operationsFor,
  signatureText,
  sqlCallable,
  windowIsDirty,
} from "./dialogs/logic";
import { Icon } from "./components/Icon";

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
  useEffect(() => setErrorTranslator(t), [t]);
  const [themeKey, setThemeKey] = useState(boot.theme.name);
  const [recents, setRecents] = useState(boot.recents);
  // Custom functions and operations (the manager, the menus).
  const [customDefs, setCustomDefs] = useState<CustomDefinition[]>([]);
  /** The usable custom functions, for the expression editor and formula bar. */
  const [customFns, setCustomFns] = useState<FunctionDef[]>([]);
  const brokenNoticed = useRef(false);
  const reloadCustom = useCallback(() => {
    api
      .customList()
      .then((l) => {
        const defs = customDefinitions(l.entries);
        setCustomDefs(defs);
        setCustomFns(l.functions ?? []);
        // After an update some may need attention: say so once.
        const broken = defs.filter((d) => d.status === "broken").length;
        if (broken && !brokenNoticed.current) {
          brokenNoticed.current = true;
          setStatusRef.current?.(broken);
        }
      })
      .catch(() => undefined);
  }, []);
  const setStatusRef = useRef<((broken: number) => void) | null>(null);
  useEffect(() => reloadCustom(), [reloadCustom]);
  /** AI clients connected through `parvu mcp --attach`. */
  const [aiClients, setAiClients] = useState(0);
  // Built-in AI assistant: panel, providers, one chat per tab (key 0: no tab).
  /** View choices (panels shown or collapsed, the query panel's height)
   * are settings, so the next start looks the same. */
  const setView = useCallback((patch: Partial<Settings>) => {
    setSettings((s) => ({ ...s, ...patch }));
    void api.updateSettings(patch).catch(() => undefined);
  }, []);
  const aiOpen = settings.show_ai_panel ?? false;
  const editorOpen = settings.show_query_panel ?? true;
  const editorCollapsed = settings.query_panel_collapsed ?? true;
  const stepsOpen = settings.show_steps_panel ?? false;
  const [aiProviders, setAiProviders] = useState<ProvidersInfo | null>(null);
  const [aiProvider, setAiProvider] = useState<string | null>(null);
  const [chats, setChats] = useState<Record<number, ChatState>>({});
  const updateChat = useCallback((key: number, f: (c: ChatState) => ChatState) => {
    setChats((all) => ({ ...all, [key]: f(all[key] ?? emptyChat()) }));
  }, []);

  const [tabs, setTabs] = useState<TabSummary[]>([]);
  const [active, setActive] = useState<number | null>(null);
  /** The active tab right now (a prompt may switch tabs while a handler waits). */
  const activeRef = useRef<number | null>(null);
  activeRef.current = active;
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
  // The column formula being written in the formula bar, if any.
  const [formulaEdit, setFormulaEdit] = useState<{ name: string; isNew: boolean; after: string | null; source: string; error: string; errorAt?: number | null } | null>(null);
  const [currentCell, setCurrentCell] = useState<{ row: number; col: number } | null>(null);
  const formulaBar = useRef<FormulaBarHandle>(null);
  // Workbooks the user agreed to overwrite with values this session.
  const workbookConfirmed = useRef(new Set<string>());
  // "Formulas are columns" is said once a session.
  const formulaHintShown = useRef(false);
  /** SQL mode: run the query into a new tab instead of changing this one. */
  const [sqlToNewTab, setSqlToNewTab] = useState(false);
  const saveAsRef = useRef<(tab: TabSummary) => Promise<boolean>>(async () => false);
  // How each column of the active tab's CSV file is read ("Read as").
  const [sourceCols, setSourceCols] = useState<SourceColumn[]>([]);
  const [busy, setBusy] = useState(false);
  const [statusText, setStatusText] = useState(t("status.ready"));
  const statusTimer = useRef<number | undefined>(undefined);
  const [dialogs, setDialogs] = useState<{ id: number; node: ReactNode }[]>([]);
  const dialogSeq = useRef(0);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuEntry[] } | null>(null);
  const [editorHeight, setEditorHeight] = useState(() => settings.query_panel_height || 220);
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
  const sourceKey = activeTab ? `${activeTab.id}:${activeTab.path}:${activeTab.steps.length}` : "";
  useEffect(() => {
    if (!activeTab) {
      setSourceCols([]);
      return;
    }
    let live = true;
    api
      .sourceColumns(activeTab.id)
      .then((cols) => live && setSourceCols(cols))
      .catch(() => live && setSourceCols([]));
    return () => {
      live = false;
    };
    // sourceKey covers the tab, its file and its steps.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sourceKey]);
  const allMethods = useMemo(() => [...new Set(Object.values(boot.methods).flat())].sort(), [boot.methods]);

  // ------------------------------------------------------------ ui helpers
  // A message shown for a while (`ms`) is not replaced by a plain one, such
  // as the page info after loading: that one waits until the time is up.
  const statusHold = useRef<{ until: number; next: string }>({ until: 0, next: "" });
  const status = useCallback((text: string, ms?: number) => {
    const hold = statusHold.current;
    if (!ms && Date.now() < hold.until) {
      hold.next = text;
      return;
    }
    window.clearTimeout(statusTimer.current);
    setStatusText(text);
    if (ms) {
      hold.until = Date.now() + ms;
      hold.next = "";
      statusTimer.current = window.setTimeout(() => {
        hold.until = 0;
        setStatusText(hold.next);
      }, ms);
    }
  }, []);
  setStatusRef.current = (broken: number) => status(t("custom.status.broken", { count: broken }), 10000);

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
      setSchema((old) => ({
        ...old,
        tables: cat.map(([name, cols]) => ({ name, columns: cols.map((c) => c.name) })),
        methods: allMethods,
      }));
    } catch {
      /* autocomplete is best-effort */
    }
  }, [allMethods]);
  // Custom functions complete beside the built-ins.
  useEffect(() => setSchema((old) => ({ ...old, functions: [...boot.functions, ...customFns] })), [boot.functions, customFns]);
  // …and in the SQL editor, where SQL mode expands their calls.
  const sqlFunctions = useMemo(
    () =>
      sqlCallable(customDefs).map((d) => ({
        name: d.name,
        detail: t(d.result.kind === "value" ? "editor.custom_summary" : "editor.custom_function"),
        signature: signatureText(d),
        description: d.description || d.title || "",
      })),
    [customDefs, t],
  );

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
    async (path: string, options?: CsvOptions, sheet?: string) => {
      setBusy(true);
      status(t("status.loading"));
      try {
        const s = await api.openFile(path, options, sheet);
        tabUi.current.set(s.id, newTabUi(s.name));
        upsertTab(s);
        switchTo(s.id);
        void refreshSchema();
        const loaded = t("status.loaded", { filename: path.split(/[\\/]/).pop() ?? path, rows: s.total_rows });
        // Formulas made for this file before come back with it.
        const broken = s.formulas.filter((f) => f.error).length;
        const restored = s.formulas.length ? `${loaded} ${t("status.formulas_restored", { count: s.formulas.length })}` : loaded;
        status(broken ? `${restored} ${t("status.formulas_broken", { count: broken })}` : restored, broken ? 10000 : 6000);
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

  /** File → Open with options: a CSV file with a delimiter, header, rows to skip… */
  const browseWithOptions = useCallback(async () => {
    const picked = await openDialog({
      multiple: false,
      title: t("open_options.title"),
      filters: [
        { name: "CSV", extensions: ["csv", "tsv", "txt"] },
        { name: "All files", extensions: ["*"] },
      ],
    });
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (!path) return;
    const options = await openOpenOptions(ui, path);
    if (options) await openPath(path, options);
  }, [openPath, t, ui]);

  // ------------------------------------------------------------ actions
  const withTab = <T,>(fn: (tab: TabSummary) => T): T | undefined => (activeTab ? fn(activeTab) : undefined);

  /** Write a result too big for memory to a file the user picks, then open it. */
  const resultToFile = useCallback(
    async (tab: TabSummary, derivation: Derivation, limit: number) => {
      if (!(await ui.confirm(t("table.too_big_title"), t("table.too_big_msg", { limit })))) return;
      const path = await saveDialog({
        title: t("dialog.save_as"),
        defaultPath: `${tab.name}_result.parquet`,
        filters: [
          { name: "Parquet", extensions: ["parquet"] },
          { name: "CSV", extensions: ["csv", "tsv"] },
        ],
      });
      if (!path) return;
      try {
        const rows = await jobResult(api.tableToFile(tab.id, derivation, path, { label: t("table.writing") }));
        if (rows !== null) await openPath(path);
      } catch (e) {
        await showError(t("error.operation_failed"), e);
      }
    },
    [ui, t, openPath, showError],
  );

  /** A table operation or query: its result opens in a new tab, computed as a job. */
  const makeTable = useCallback(
    async (tab: TabSummary, derivation: Derivation): Promise<TabSummary | null> => {
      let r;
      try {
        r = await jobResult(api.makeTable(tab.id, derivation, { label: t("table.computing") }));
      } catch (e) {
        await showError(t("error.operation_failed"), e);
        return null;
      }
      if (!r) return null;
      if (r.kind === "too_big") {
        await resultToFile(tab, derivation, r.limit);
        return null;
      }
      const s = r.summary;
      tabUi.current.set(s.id, newTabUi(s.name));
      upsertTab(s);
      switchTo(s.id);
      void refreshSchema();
      status(t("table.opened", { name: s.name, source: tab.name }), 6000);
      return s;
    },
    [t, showError, resultToFile, upsertTab, switchTo, refreshSchema, status],
  );

  /** Compute a result tab again from its source (↻ on its chip). */
  const refreshResult = useCallback(
    async (tab: TabSummary) => {
      if ((tab.pending_edits || tab.steps.length) && !(await ui.confirm(t("table.refresh_title"), t("table.refresh_msg", { name: tab.name }))))
        return;
      let r;
      try {
        r = await jobResult(api.refreshTable(tab.id, { label: t("table.refreshing", { name: tab.name }) }));
      } catch (e) {
        await showError(t("error.operation_failed"), e);
        return;
      }
      if (!r) return;
      if (r.kind === "too_big") {
        await ui.alert(t("table.too_big_title"), t("table.too_big_refresh", { limit: r.limit }), "warning");
        return;
      }
      upsertTab(r.summary);
      const st = tabUi.current.get(tab.id);
      if (st) st.sort = null;
      if (tab.id === active) await loadPage(tab.id, 1);
      void refreshSchema();
      status(t("table.refreshed", { name: tab.name }), 5000);
    },
    [ui, t, showError, upsertTab, active, loadPage, refreshSchema, status],
  );

  const execute = useCallback(async () => {
    if (!activeTab) return;
    const text = editorText.trim();
    if (!text) {
      await ui.alert(t("error.empty_query"), t("error.empty_query_msg"), "warning");
      return;
    }
    if (mode === "sql" && sqlToNewTab) {
      await makeTable(activeTab, { kind: "sql", sql: text });
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
  }, [activeTab, editorText, mode, mutate, makeTable, sqlToNewTab, t, ui]);

  const applyOp = useCallback(
    async (op: Operation) => {
      if (!activeTab) return;
      if (isTableOp(op)) return makeTable(activeTab, { kind: "op", op });
      return mutate(activeTab.id, () => api.applyOp(activeTab.id, op), t("error.operation_failed"));
    },
    [activeTab, mutate, makeTable, t],
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
    // Reset drops every step and can't be undone: ask first.
    // Cell edits, formulas and "Read as" choices stay; the rest goes.
    const kept = (k: string) => k.startsWith("step.cell_edit") || k.startsWith("step.formula") || k === "step.source_type";
    const sqlSteps = activeTab.steps.filter((x) => !kept(x.key)).length;
    if (sqlSteps && !(await ui.confirm(t("dialog.reset.title", { name: activeTab.name }), t("dialog.reset.message", { count: sqlSteps })))) return;
    const id = activeTab.id;
    const s = await mutate(id, () => api.resetTab(id), t("error.query_error"));
    if (s) {
      // The tab's own query (the whole table), not the "default query"
      // setting, which may carry a LIMIT that would cut the table.
      const q = s.current_query;
      const st = tabUi.current.get(id);
      if (st) {
        st.query = q;
        st.sort = null;
      }
      setEditorText(q);
      status(t("status.reset"), 3000);
    }
  }, [activeTab, mutate, t, settings, status, ui]);

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
      if (!activeTab) return;
      // The dialog opens at once and fills while the column is read piece
      // by piece; the grid stays usable meanwhile.
      const chosen = await openUniqueValues(ui, { tabId: activeTab.id, column, total: activeTab.total_rows });
      if (chosen && chosen.length) await applyOp({ op: "filter_values", column, values: chosen });
    },
    [activeTab, ui, applyOp],
  );

  const showStats = useCallback(
    async (column: string) => {
      if (!activeTab) return;
      try {
        const job = api.columnStats(activeTab.id, column, { label: t("job.column_stats", { column }), total: activeTab.total_rows });
        const stats = await jobResult(job);
        if (stats) await openColumnStats(ui, { stats });
      } catch (e) {
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
        let text =
          r.mode === "page"
            ? pythonTuple(pageValues, numeric)
            : await jobResult(api.columnTuple(activeTab.id, column, r.mode, r.n, { label: t("job.tuple", { column }) }));
        if (text === null) return;
        if (r.label) text = `${r.label} = ${text}`;
        await copyText(text, t("status.copied", { count: r.mode === "page" ? pageValues.length : r.n }));
      } catch (e) {
        await showError(t("error.operation_failed"), e);
      }
    },
    [activeTab, page, settings, ui, copyText, t, showError],
  );

  const columnNames = page?.columns.map((c) => c.name) ?? [];
  /** Frequent values of a column, suggested in condition value boxes;
   * fetched once per column and query. */
  const suggestCache = useRef(new Map<string, Promise<string[]>>());
  const suggestValues = (column: string): Promise<string[]> => {
    if (!activeTab) return Promise.resolve([]);
    const key = `${activeTab.id}\u0000${activeTab.current_query}\u0000${activeTab.pending_edits}\u0000${column}`;
    let p = suggestCache.current.get(key);
    if (!p) {
      p = jobResult(api.valueSuggestions(activeTab.id, column))
        .then((v) => v ?? [])
        .catch(() => []);
      suggestCache.current.set(key, p);
    }
    return p;
  };

  const runConvert = async (column: string) => {
    if (!activeTab) return;
    const op = await openConvert(ui, { tabId: activeTab.id, table: activeTab.name, column });
    if (op) await applyOp(op);
  };

  const runColumnFn = async (column: string, name: FnName) => {
    if (!activeTab || !page) return;
    const i = page.columns.findIndex((c) => c.name === column);
    const values = i >= 0 ? previewValues(page.rows, i) : [];
    const op = await openColumnFn(ui, { tabId: activeTab.id, table: activeTab.name, column, name, values, columns: page.columns, suggest: suggestValues });
    if (op) await applyOp(op);
  };

  /** JSON, list and struct columns: expand, extract, to rows, length, join. */
  const nestedMenu = (column: string, family: "json" | "list" | "struct"): MenuEntry[] => {
    if (family === "struct")
      return [{ label: t("json.expand_struct"), onSelect: () => void applyOp({ op: "expand_struct", column }) }, { type: "separator" }];
    const listItems: MenuEntry[] = [
      { label: t("json.to_rows"), onSelect: () => void applyOp({ op: "to_rows", column }) },
      {
        label: t("fn.list_length"),
        onSelect: () => void applyOp({ op: "column_transform", column, func: { fn: "list_length" }, output: { to: "new", name: "" } }),
      },
      { label: `${t("fn.join_list")}…`, onSelect: () => void runColumnFn(column, "join_list") },
    ];
    if (family === "list") return [...listItems, { type: "separator" }];
    return [
      {
        label: t("menu.col.json"),
        icon: "json" as const,
        submenu: [
          {
            label: `${t("json.expand.title")}…`,
            onSelect: async () => {
              if (!activeTab) return;
              const op = await openExpandJson(ui, { tabId: activeTab.id, table: activeTab.name, column });
              if (op) await applyOp(op);
            },
          },
          { label: `${t("fn.json_path")}…`, onSelect: () => void runColumnFn(column, "json_path") },
          { type: "separator" as const },
          ...listItems,
        ],
      },
      { type: "separator" },
    ];
  };

  /** Date and time columns: parts, names, start/end of, age, shifts, differences, relative filters. */
  const dateMenu = (column: string, timeOnly: boolean): MenuEntry[] => {
    const family = page?.columns.find((c) => c.name === column)?.family;
    const toNew = (func: ColumnFn) => void applyOp({ op: "column_transform", column, func, output: { to: "new", name: "" } });
    const inPlace = (func: ColumnFn) => void applyOp({ op: "column_transform", column, func, output: { to: "in_place" } });
    const part = (p: DatePartName) => ({ label: t(`part.${p}`), onSelect: () => toNew({ fn: "date_part", part: p }) });
    const names = (key: string, n: number) => Array.from({ length: n }, (_, i) => t(`${key}.${i + 1}`));
    const timeEntries: MenuEntry[] = [part("hour"), part("minute"), part("second")];
    if (timeOnly) return [{ label: t("menu.col.time"), icon: "date" as const, submenu: timeEntries }, { type: "separator" }];
    const dateCols = (page?.columns ?? []).filter((c) => isDateFamily(c.family)).map((c) => c.name);
    return [
      {
        label: t("menu.col.date"),
        icon: "date" as const,
        submenu: [
          part("year"),
          part("quarter"),
          part("month"),
          { label: t("part.month_name"), onSelect: () => toNew({ fn: "month_name", names: names("date.month", 12) }) },
          part("day"),
          { label: t("part.day_name"), onSelect: () => toNew({ fn: "day_name", names: names("date.day", 7) }) },
          part("day_of_week"),
          part("week"),
          part("day_of_year"),
          { type: "separator" as const },
          { label: t("fn.is_weekend"), onSelect: () => toNew({ fn: "is_weekend" }) },
        ],
      },
      ...(hasTime(family)
        ? [
            {
              label: t("menu.col.time"),
              icon: "date" as const,
              submenu: [
                ...timeEntries,
                { type: "separator" as const },
                { label: t("fn.date_only"), onSelect: () => inPlace({ fn: "date_only" }) },
                { label: t("fn.time_only"), onSelect: () => toNew({ fn: "time_only" }) },
              ],
            },
          ]
        : []),
      {
        label: t("menu.col.start_of"),
        icon: "date" as const,
        submenu: (["year", "quarter", "month", "week", "day", ...(hasTime(family) ? ["hour"] : [])] as TimeUnitName[]).map((u) => ({
          label: t(`unit.${u}`),
          onSelect: () => inPlace({ fn: "start_of", unit: u }),
        })),
      },
      {
        label: t("menu.col.end_of"),
        icon: "date" as const,
        submenu: (["month", "quarter", "year"] as TimeUnitName[]).map((u) => ({
          label: t(`unit.${u}`),
          onSelect: () => inPlace({ fn: "end_of", unit: u }),
        })),
      },
      {
        label: t("menu.col.age"),
        icon: "date" as const,
        submenu: (["day", "month", "year"] as TimeUnitName[]).map((u) => ({
          label: t(`unit.${u}s`),
          onSelect: () => toNew({ fn: "age", unit: u }),
        })),
      },
      { label: `${t("fn.add_time")}…`, onSelect: () => void runColumnFn(column, "add_time") },
      { label: `${t("fn.format_date")}…`, onSelect: () => void runColumnFn(column, "format_date") },
      {
        label: `${t("dates.diff.title")}…`,
        onSelect: async () => {
          if (!activeTab) return;
          const op = await openDateDiff(ui, { table: activeTab.name, dates: dateCols, end: column });
          if (op) await applyOp(op);
        },
      },
      {
        label: t("menu.col.filter_dates"),
        icon: "filter" as const,
        submenu: [
          ...DATE_FILTERS.map((f) => ({ label: t(f.key), onSelect: () => void applyOp({ op: "date_range", column, range: f.range }) })),
          { type: "separator" as const },
          {
            label: t("dates.last_n.title"),
            onSelect: async () => {
              const op = await openLastN(ui, column);
              if (op) await applyOp(op);
            },
          },
        ],
      },
      { type: "separator" },
    ];
  };

  /** Operations that fit the column's type (text, yes/no). */
  const typeMenu = (column: string): MenuEntry[] => {
    const family = page?.columns.find((c) => c.name === column)?.family;
    const inPlace = (func: ColumnFn) => void applyOp({ op: "column_transform", column, func, output: { to: "in_place" } });
    if (family === "text")
      return [
        {
          label: t("menu.col.text"),
          icon: "text",
          submenu: [
            ...TEXT_FNS.map((f) => ({ label: t(fnLabelKey(f)), onSelect: () => inPlace(f) })),
            { type: "separator" as const },
            ...(["add_prefix", "add_suffix", "pad"] as FnName[]).map((n) => ({
              label: `${t(`fn.${n}`)}…`,
              onSelect: () => void runColumnFn(column, n),
            })),
            { type: "separator" as const },
            {
              label: `${t("split_rows.title")}…`,
              onSelect: async () => {
                const d = await ui.prompt(t("split_rows.title"), t("split_rows.prompt"), ",");
                if (d) await applyOp({ op: "split_to_rows", column, delimiter: d, trim: true });
              },
            },
          ],
        },
        {
          label: t("menu.col.extract"),
          icon: "extract",
          submenu: EXTRACT_FNS.map((n) => ({
            label: n === "length" ? t("fn.length") : `${t(`fn.${n}`)}…`,
            onSelect: () =>
              n === "length"
                ? void applyOp({ op: "column_transform", column, func: { fn: "length" }, output: { to: "new", name: "" } })
                : void runColumnFn(column, n),
          })),
        },
        { label: t("menu.col.merge"), icon: "merge", onSelect: () => void runOpDialog(openMergeColumns, column) },
        { type: "separator" },
      ];
    if (isDateFamily(family) || family === "time") return dateMenu(column, family === "time");
    if (family === "json" || family === "list" || family === "struct") return nestedMenu(column, family);
    if (family === "boolean")
      return [
        { label: t("fn.invert"), icon: "convert", onSelect: () => inPlace({ fn: "invert" }) },
        { label: t("menu.col.keep_true"), icon: "filter", onSelect: () => void applyOp({ op: "filter_values", column, values: ["true"] }) },
        { label: t("menu.col.keep_false"), icon: "filter", onSelect: () => void applyOp({ op: "filter_values", column, values: ["false"] }) },
        { label: t("menu.col.keep_empty"), icon: "filter", onSelect: () => void applyOp({ op: "filter_values", column, values: [null] }) },
        { type: "separator" },
      ];
    return [];
  };

  const readAsMenu = (column: string): MenuEntry[] => {
    const src = sourceCols.find((c) => c.name === column);
    if (!src || !activeTab) return [];
    const id = activeTab.id;
    const read = (reading: Reading) => void mutate(id, () => api.setSourceReading(id, column, reading), t("error.operation_failed"));
    const entries: MenuEntry[] = [];
    const r = src.reading;
    if (r.kind === "date_format" || r.kind === "timestamp_format") {
      // Dates such as 03/04/2024: flip day and month in one click. Only
      // when the file's dates allow both orders (no day above 12): the
      // other order would turn the rest into empty cells.
      const other = swapDayMonth(r.format);
      if (other !== r.format && src.ambiguous)
        entries.push({
          label: t(dayFirst(r.format) ? "types.month_first" : "types.day_first"),
          onSelect: () => read({ ...r, format: other }),
        });
    }
    entries.push({
      label: t("types.read_as"),
      icon: "type",
      submenu: readAsChoices().map((c) => ({
        label: t(c.key, c.args),
        checked: sameReading(c.reading, r),
        onSelect: () => read(c.reading),
      })),
    });
    entries.push({ type: "separator" });
    return entries;
  };

  const formulaMenu = (column: string): MenuEntry[] =>
    formulaNames.includes(column)
      ? [
          { label: t("formula.edit"), icon: "formula", onSelect: () => editFormula(column) },
          { label: t("formula.remove"), icon: "removeColumn", onSelect: () => void removeFormula(column) },
        ]
      : [{ label: t("formula.new_after"), icon: "formula", onSelect: () => startFormula(column) }];

  /** The Column menu: what adds or changes a column, for `column`, in named
   * groups. The menu bar's Column menu and the header's right-click menu
   * share it. `extraClean` adds row-dropping items to the Clean group. */
  const columnMenu = (column: string, extraClean: MenuEntry[] = []): MenuEntry[] => {
    // Functions for this column's type: their submenus (Text, Extract,
    // Dates…) stay at the top; single items go to Transform.
    type Item = Exclude<MenuEntry, { type: "separator" }>;
    const typed = typeMenu(column).filter((e): e is Item => e.type !== "separator");
    const typedGroups = typed.filter((e) => e.submenu);
    const typedItems = typed.filter((e) => !e.submenu && e.label !== t("menu.col.merge"));
    return [
      ...formulaMenu(column),
      { type: "separator" },
      {
        label: t("menu.group.column"),
        icon: "column",
        submenu: [
          {
            label: t("context.rename_column"),
            icon: "rename",
            onSelect: async () => {
              const name = await ui.prompt(t("context.rename_dialog.title"), t("context.rename_dialog.prompt"), column);
              if (name?.trim() && name.trim() !== column) await applyOp({ op: "rename", column, new_name: name.trim() });
            },
          },
          { label: t("context.duplicate_column"), icon: "duplicate", onSelect: () => void applyOp({ op: "duplicate", column }) },
          {
            label: t("context.remove_column"),
            icon: "removeColumn",
            disabled: columnNames.length < 2,
            onSelect: () => void applyOp({ op: "remove", column }),
          },
          { type: "separator" },
          { label: t("context.copy_column"), icon: "copy", onSelect: () => void copyText(column, t("status.copied_name")) },
        ],
      },
      {
        label: t("menu.group.type"),
        icon: "dtype",
        submenu: [
          ...readAsMenu(column),
          { label: t("context.convert"), icon: "convert", onSelect: () => void runConvert(column) },
          { type: "separator" as const },
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
      ...typedGroups,
      {
        label: t("menu.group.transform"),
        icon: "transform",
        submenu: [
          ...typedItems,
          ...(typedItems.length ? [{ type: "separator" as const }] : []),
          { label: t("menu.data.split"), icon: "split", onSelect: () => void runOpDialog(openSplitColumn, column) },
          { label: t("menu.data.merge"), icon: "merge", onSelect: () => void runOpDialog(openMergeColumns, column) },
          { label: t("menu.data.bin"), icon: "bin", onSelect: () => void runOpDialog(openBin, column) },
          { label: t("menu.operations.window"), icon: "window", onSelect: () => void runOpDialog(openWindow, column, true) },
          { label: t("menu.operations.math"), icon: "math", onSelect: () => void runMath() },
        ],
      },
      {
        label: t("custom.menu.functions"),
        icon: "custom",
        submenu: [
          ...customEntries(functionsForColumn(customDefs, page?.columns.find((c) => c.name === column)?.family), column),
          ...(functionsForColumn(customDefs, page?.columns.find((c) => c.name === column)?.family).length ? [{ type: "separator" as const }] : []),
          { label: t("custom.menu.new_function"), icon: "plus", onSelect: () => void newCustom("function") },
          { label: t("custom.menu.manage"), icon: "custom", onSelect: manageCustom },
        ],
      },
      {
        label: t("menu.group.clean"),
        icon: "clean",
        submenu: [
          { label: t("set_value.menu"), icon: "edit", onSelect: () => void runOpDialog(openSetValue, column) },
          { label: t("context.replace_values"), icon: "replace", onSelect: () => void runReplace(column) },
          { label: t("context.fill_null"), icon: "fill", onSelect: () => void runOpDialog(openFillNull, column) },
          { type: "separator" },
          { label: t("menu.operations.drop_duplicates"), icon: "dropDuplicates", onSelect: () => void runDropDuplicates(column) },
          { label: t("menu.operations.mark_duplicates"), icon: "markDuplicates", onSelect: () => void runDropDuplicates(column, true) },
          ...extraClean,
        ],
      },
      {
        label: t("menu.group.analyze"),
        icon: "stats",
        submenu: [
          { label: t("context.unique_values"), icon: "unique", onSelect: () => void showUniqueValues(column) },
          { label: t("context.column_stats"), icon: "info", onSelect: () => void showStats(column) },
          { label: t("context.chart"), icon: "chart", onSelect: () => void showChart(column) },
          { label: t("context.copy_values"), icon: "copy", onSelect: () => void copyTuple(column) },
        ],
      },
    ];
  };

  /** Right-click on a header: sorting and filtering first, then the Column
   * menu's groups, tables made from the column, and how it looks. */
  const headerMenu = (column: string): MenuEntry[] => [
    { label: t("context.sort_asc"), icon: "sortAsc", onSelect: () => void sortBy(column, true) },
    { label: t("context.sort_desc"), icon: "sortDesc", onSelect: () => void sortBy(column, false) },
    { label: t("context.filter"), icon: "filter", onSelect: () => void runOpDialog(openFilter, column) },
    { type: "separator" },
    ...columnMenu(column, [{ label: t("context.drop_null"), icon: "dropNull", onSelect: () => void runDropNull(column) }]),
    {
      label: t("context.new_table_from"),
      icon: "newTable",
      submenu: [
        { label: t("menu.reshape.group_by"), icon: "groupBy", onSelect: () => void runOpDialog(openGroupByCustom, column, true) },
        { label: t("menu.reshape.pivot"), icon: "pivot", onSelect: () => void runOpDialog(openPivot, column, true) },
        { label: t("menu.reshape.value_counts"), icon: "valueCounts", onSelect: () => void runOpDialog(openValueCounts, column, true) },
      ],
    },
    {
      label: t("menu.group.display"),
      icon: "view",
      submenu: [
        {
          label: t("context.freeze_here"),
          icon: "pin",
          onSelect: () => activeTab && updateView(activeTab.id, { frozen: columnNames.indexOf(column) + 1 }),
        },
        {
          label: t("context.unfreeze"),
          icon: "unpin",
          disabled: !(activeTab && (tabUi.current.get(activeTab.id)?.frozen ?? 0) > 0),
          onSelect: () => activeTab && updateView(activeTab.id, { frozen: 0 }),
        },
        { type: "separator" },
        { label: t("context.conditional_format"), icon: "format", onSelect: () => void editFormats(column) },
        {
          label: t("context.clear_format"),
          icon: "clearFormat",
          disabled: !(activeTab && tabUi.current.get(activeTab.id)?.formats.some((f) => f.column === column)),
          onSelect: () => {
            const st = activeTab && tabUi.current.get(activeTab.id);
            if (st && activeTab) updateView(activeTab.id, { formats: st.formats.filter((f) => f.column !== column) });
          },
        },
      ],
    },
  ];

  /** Fill down / up / right / left for the selection. */
  const fillMenu = (): MenuEntry[] => {
    const range = grid.current?.selectionRange();
    const rows = !page?.editable || (range?.height ?? 0) < 2;
    const cols = !page?.editable || (range?.width ?? 0) < 2;
    return [
      { label: t("context.fill_down"), icon: "fillDown", shortcut: "Ctrl+D", disabled: rows, onSelect: () => void fillRange("down") },
      { label: t("context.fill_up"), icon: "fillUp", disabled: rows, onSelect: () => void fillRange("up") },
      { label: t("context.fill_right"), icon: "fillRight", shortcut: "Ctrl+R", disabled: cols, onSelect: () => void fillRange("right") },
      { label: t("context.fill_left"), icon: "fillLeft", disabled: cols, onSelect: () => void fillRange("left") },
      { type: "separator" },
      { label: t("context.fill_series"), icon: "series", disabled: rows && cols, onSelect: () => void runSeries() },
    ];
  };

  const cellMenu = (row: number, col: number): MenuEntry[] => {
    const copy = (format: "tsv" | "csv" | "markdown", headers = false) => {
      // Read the selection when the item is chosen: a right-click outside
      // the selection selects the clicked cell, and that state isn't in
      // place yet while the menu is being built.
      const sel = grid.current?.selection();
      if (!sel) return;
      void copyText(
        formatRows(sel.rows, format, headers || format === "markdown" ? sel.columns : undefined),
        t("status.copied_cells", { rows: sel.rows.length, cols: sel.columns.length }),
      );
    };
    const value = page?.rows[row]?.[col] ?? null;
    const column = page?.columns[col]?.name ?? "";
    return [
      { label: t("context.copy"), icon: "copy", shortcut: "Ctrl+C", onSelect: () => copy("tsv") },
      {
        label: t("context.copy_as"),
        icon: "paste",
        submenu: [
          { label: t("context.copy_csv"), onSelect: () => copy("csv") },
          { label: t("context.copy_markdown"), onSelect: () => copy("markdown") },
          { label: t("context.copy_with_headers"), onSelect: () => copy("tsv", true) },
        ],
      },
      { type: "separator" },
      { label: t("context.filter_by_value"), icon: "filter", onSelect: () => void applyOp({ op: "filter_values", column, values: [value] }) },
      { label: t("context.filter_custom"), icon: "filter", onSelect: () => void runFilterFromCell(column, value) },
      { type: "separator" },
      { label: t("context.fill"), icon: "fill", disabled: !page?.editable, submenu: fillMenu() },
      {
        label: t("context.set_null"),
        icon: "clearFormat",
        disabled: !page?.editable || value === null,
        onSelect: () => void editCell(row, column, null),
      },
    ];
  };

  const editCell = useCallback(
    async (row: number, column: string, value: Cell) => {
      if (!activeTab || !page) return;
      // "=…" in a cell is a value: formulas are columns. Say so once.
      const formulaHint = !!value?.trim().startsWith("=") && !formulaHintShown.current;
      if (formulaHint) formulaHintShown.current = true;
      if (!page.editable) {
        status(t("status.read_only"), 4000);
        return;
      }
      try {
        const s = await api.editCell(activeTab.id, page.offset + row, column, value);
        upsertTab(s);
        await loadPage(activeTab.id, page.page);
        if (formulaHint) status(t("status.formula_hint"), 9000);
      } catch (e) {
        await loadPage(activeTab.id, page.page);
        const rejected = t("status.edit_rejected", { error: errorMessage(e) });
        status(formulaHint ? `${rejected} ${t("status.formula_hint")}` : rejected, 9000);
      }
    },
    [activeTab, page, upsertTab, loadPage, status, t],
  );

  // ------------------------------------------------------------ formulas
  const formulas = activeTab?.formulas ?? [];
  const formulaNames = formulas.map((f) => f.name);
  const brokenFormulas = useMemo(() => formulas.filter((f) => f.error).map((f) => f.name), [formulas]);
  /** The formula bar completes a bare `[` with this table's columns. */
  const formulaSchema = useMemo<ExprSchema>(() => ({ ...schema, bare: page?.columns.map((c) => c.name) ?? [] }), [schema, page]);
  /** Start a new formula column after `after` (default: the selected column). */
  /** Start a new formula column after `after`; left out: after the
   * selected cell's column; `null`: at the end. */
  const startFormula = (after?: string | null, source = "") => {
    const cols = page?.columns.map((c) => c.name) ?? [];
    const at = after !== undefined ? after : currentCell ? (cols[currentCell.col] ?? null) : null;
    setFormulaEdit({ name: newFormulaName([...cols, ...formulaNames]), isNew: true, after: at, source, error: "" });
  };
  const editFormula = (name: string) => {
    const f = formulas.find((x) => x.name === name);
    // A formula kept with an error opens with it, to be fixed.
    if (f) setFormulaEdit({ name: f.name, isNew: false, after: f.after, source: f.source, error: f.error ? t("formula.broken", { error: f.error }) : "" });
  };
  const applyFormula = async () => {
    const fe = formulaEdit;
    if (!activeTab || !fe) return;
    try {
      const s = await api.setFormula(activeTab.id, fe.name.trim(), fe.source, fe.after);
      setFormulaEdit(null);
      upsertTab(s);
      await loadPage(activeTab.id, page?.page ?? 1);
      void refreshSchema();
      const last = s.steps[s.steps.length - 1];
      if (last) status(t("steps.applied", { step: stepText(t, last) }), 4000);
    } catch (e) {
      // Parse errors carry a position: underline it in the bar.
      const pos = e as { line?: number; column?: number };
      const errorAt = pos.line ? errorOffset(fe.source, pos.line, pos.column ?? 1) : null;
      setFormulaEdit({ ...fe, error: errorMessage(e), errorAt });
    }
  };
  // ------------------------------------------------------------ Excel sheets
  /** The sheet menu of the active workbook tab, under `anchor`. */
  const showSheetMenu = async (anchor: HTMLElement) => {
    if (!activeTab) return;
    const tab = activeTab;
    let view: SheetsView;
    try {
      view = await api.excelSheets(tab.id);
    } catch (e) {
      await showError(t("sheets.error"), e);
      return;
    }
    const refreshTabs = async () => (await api.tabs()).forEach(upsertTab);
    const run = async (fn: () => Promise<unknown>) => {
      try {
        await fn();
        await refreshTabs();
      } catch (e) {
        await showError(t("sheets.error"), e);
      }
    };
    const actions: SheetActions = {
      show: (s) =>
        void run(async () => {
          // The tab starts afresh on another sheet: offer a new tab instead
          // when it holds work.
          if (tab.steps.length || tab.pending_edits || tab.formulas.length) {
            const choice = await ui.ask(t("sheets.switch_title"), t("sheets.switch_msg", { sheet: s.name }), [
              { id: "cancel", label: t("btn.cancel") },
              { id: "switch", label: t("sheets.switch_anyway"), danger: true },
              { id: "new_tab", label: t("sheets.open_in_tab"), primary: true },
            ]);
            if (choice === "new_tab") return openPath(tab.path, undefined, s.source);
            if (choice !== "switch") return;
          }
          const summary = await api.setSheet(tab.id, s.source);
          tabUi.current.set(tab.id, newTabUi(summary.name));
          upsertTab(summary);
          await loadPage(tab.id, 1);
          void refreshSchema();
          status(t("sheets.switched", { sheet: s.name }), 4000);
        }),
      openInTab: (s) => void openPath(tab.path, undefined, s.source),
      openAll: () =>
        void (async () => {
          for (const s of sheetsToOpen(view)) await openPath(tab.path, undefined, s.source);
        })(),
      rename: (s) =>
        void run(async () => {
          const name = await ui.prompt(t("sheets.rename_title"), t("sheets.rename_label"), s.name);
          if (name !== null && name.trim() !== s.name) await api.sheetOp(tab.id, { op: "rename", sheet: s.name, name });
        }),
      op: (op) => void run(() => api.sheetOp(tab.id, op)),
    };
    const r = anchor.getBoundingClientRect();
    setMenu({ x: r.left, y: r.bottom + 4, items: sheetMenu(t, view, actions) });
  };

  const removeFormula = async (name: string) => {
    if (!activeTab) return;
    await mutate(activeTab.id, () => api.removeFormula(activeTab.id, name), t("error.operation_failed"));
  };
  // While writing a formula, its references are coloured in the grid too.
  const darkTheme = useMemo(() => isDark(resolvedColor("var(--md-surface)", "#ffffff")), [themeKey]);
  const formulaHighlights = useMemo(
    () =>
      formulaEdit ? columnHighlights(formulaRefs(formulaEdit.source), (page?.columns ?? []).map((c) => c.name), darkTheme) : undefined,
    [formulaEdit, darkTheme, page],
  );
  // What the bar shows while not editing: the formula of a formula column,
  // or the selected cell's value.
  const runReplace = useCallback(
    async (selected?: string) => {
      if (!activeTab) return;
      const req = await openReplace(ui, { table: activeTab.name, columns: columnNames, selected, meta: page?.columns, suggest: suggestValues });
      if (req) await mutate(activeTab.id, () => api.replaceValues(activeTab.id, req), t("error.operation_failed"));
    },
    [activeTab, ui, columnNames, mutate, t, page],
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

  /** Drop duplicated rows, or (`mark`) add a yes/no column for them. */
  const runDropDuplicates = async (selected?: string, mark = false) =>
    withTab(async (tab) => {
      const op = await openDropDuplicates(ui, { table: tab.name, columns: columnNames, selected, mark });
      if (op) await applyOp(op);
    });

  const runMath = async () =>
    withTab(async (tab) => {
      const selected = currentCell ? page?.columns[currentCell.col]?.name : undefined;
      const r = await openMathDialog(ui, { tabId: tab.id, table: tab.name, columns: columnNames, selected });
      if (r) await mutate(tab.id, () => api.addComputedColumn(tab.id, r.name, r.text), t("error.operation_failed"));
    });

  /** Open an operation dialog for the active tab and apply its result. */
  type OpDialog = (
    ui: Ui,
    a: { table: string; columns: ColumnMeta[]; selected?: string; suggest?: (column: string) => Promise<string[]> },
  ) => Promise<Operation | null>;
  const runOpDialog = async (open: OpDialog, selected?: string, expensive = false) =>
    withTab(async (tab) => {
      if (expensive && !(await confirmLargeDataset())) return;
      const op = await open(ui, { table: tab.name, columns: page?.columns ?? [], selected, suggest: suggestValues });
      if (!op) return;
      const s = await applyOp(op);
      // A multi-column sort replaces the header sort indicator.
      const st = tabUi.current.get(tab.id);
      if (s && st && op.op === "sort_by") st.sort = null;
    });

  /** Apply a custom function or operation from a menu. A summary
   * function's dialog shows its value; the others return an operation. */
  const runCustom = async (def: CustomDefinition, selected?: string) =>
    withTab(async (tab) => {
      if (def.kind === "operation" && operationsFor([def], true).length && !(await confirmLargeDataset())) return;
      const cat = def.params.some((p) => p.kind === "table") ? await api.catalog() : [];
      const others = cat.filter(([n]) => n !== tab.name).map(([name, columns]) => ({ name, columns }));
      const op = await openCustomApply(ui, {
        tabId: tab.id,
        table: tab.name,
        def,
        columns: page?.columns ?? [],
        selected,
        others,
        suggest: suggestValues,
      });
      if (op) await applyOp(op);
    });

  /** An AI client proposed a custom tool: the user may review it in the
   * editor and save it there; nothing is saved otherwise. */
  const reviewCustomOffer = async (offer: CustomOffer) => {
    const name = offer.definition.name;
    const args = { client: offer.client, name, rationale: offer.rationale ?? "" };
    const message = [
      t(offer.passed ? "custom.offer.passed" : "custom.offer.failed", args),
      offer.rationale ? t("custom.offer.rationale", args) : "",
      t("custom.offer.review"),
    ]
      .filter(Boolean)
      .join("\n\n");
    if (!(await ui.confirm(t("custom.offer.title", args), message))) return;
    try {
      const l = await api.customList();
      const note = t("custom.offer.note", args);
      if (await openCustomEditor(ui, { definition: offer.definition, editing: offer.replaces, typeSets: l.type_sets, valueTypes: l.value_types, note }))
        reloadCustom();
    } catch (e) {
      await showError(t("error.operation_failed"), e);
    }
  };
  const newCustom = async (kind: "function" | "operation") => {
    try {
      const l = await api.customList();
      if (await openCustomEditor(ui, { definition: newDefinition(kind), editing: null, typeSets: l.type_sets, valueTypes: l.value_types }))
        reloadCustom();
    } catch (e) {
      await showError(t("error.operation_failed"), e);
    }
  };

  const manageCustom = () => void openCustomTools(ui).then(reloadCustom);

  /** Group By, with the summary custom functions that can aggregate. */
  const openGroupByCustom: OpDialog = (u, a) => openGroupBy(u, { ...a, custom: customAggregates(customDefs) });

  /** "Save steps as custom operation…": the steps that can be part of it,
   * columns as parameters, then the editor, prefilled, to review and save. */
  const saveStepsAsOperation = async () =>
    withTab(async (tab) => {
      try {
        const definition = await openSaveSteps(ui, { tabId: tab.id, table: tab.name });
        if (!definition) return;
        const l = await api.customList();
        const note = t("custom.steps.note", { table: tab.name });
        if (await openCustomEditor(ui, { definition, editing: null, typeSets: l.type_sets, valueTypes: l.value_types, note, noteIcon: "steps" }))
          reloadCustom();
      } catch (e) {
        await showError(t("error.operation_failed"), e);
      }
    });

  /** Menu entries for custom tools: usable ones run, drafts and broken
   * ones are shown disabled; past the limit, "More…" opens the manager. */
  const customEntries = (defs: CustomDefinition[], selected?: string): MenuEntry[] => {
    const items: MenuEntry[] = defs.slice(0, CUSTOM_MENU_MAX).map((d) => ({
      label: isUsable(d) ? d.title || d.name : t(d.status === "draft" ? "custom.menu.draft" : "custom.menu.broken", { name: d.title || d.name }),
      icon: "custom" as const,
      disabled: !isUsable(d) || !activeTab,
      onSelect: () => void runCustom(d, selected),
    }));
    if (defs.length > CUSTOM_MENU_MAX) items.push({ label: t("custom.menu.more"), onSelect: manageCustom });
    return items;
  };

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
        suggest: suggestValues,
      });
      if (op) await applyOp(op);
    });

  /** Write filled cells as one edit step. Formula columns and unchanged cells are skipped. */
  const writeFill = async (cells: FillCell[]) =>
    withTab(async (tab) => {
      if (!page) return;
      if (!page.editable) {
        status(t("status.read_only"), 4000);
        return;
      }
      const formulas = new Set(tab.formulas.map((f) => f.name));
      let skipped = false;
      const edits: { row: number; column: string; value: Cell }[] = [];
      for (const c of cells) {
        const column = page.columns[c.col]?.name;
        if (!column) continue;
        if (formulas.has(column)) {
          skipped = true;
          continue;
        }
        if ((page.rows[c.row]?.[c.col] ?? null) === c.value) continue;
        edits.push({ row: page.offset + c.row, column, value: c.value });
      }
      if (!edits.length) {
        if (skipped) status(t("status.fill_skipped_formulas"), 6000);
        return;
      }
      try {
        const s = await api.editCells(tab.id, edits);
        upsertTab(s);
        await loadPage(tab.id, page.page);
        const last = s.steps[s.steps.length - 1];
        if (skipped) status(t("status.fill_skipped_formulas"), 6000);
        else if (last) status(t("steps.applied", { step: stepText(t, last) }), 4000);
      } catch (e) {
        await loadPage(tab.id, page.page);
        status(t("status.edit_rejected", { error: errorMessage(e) }), 8000);
      }
    });

  /** Copy the selection's edge row or column over the rest (Ctrl+D down, Ctrl+R right). */
  const fillRange = async (direction: FillDirection) =>
    withTab(async () => {
      const range = grid.current?.selectionRange();
      const vertical = direction === "down" || direction === "up";
      if (!page || !range || (vertical ? range.height : range.width) < 2) {
        status(t(vertical ? "status.fill_rows_select" : "status.fill_columns_select"), 4000);
        return;
      }
      await writeFill(fillEdge((r, c) => page.rows[r]?.[c] ?? null, range, direction));
    });
  /** Series…: fill the selection from its first cell with an explicit step. */
  const runSeries = async () =>
    withTab(async () => {
      const range = grid.current?.selectionRange();
      if (!page || !range || (range.height < 2 && range.width < 2)) {
        status(t("status.fill_rows_select"), 4000);
        return;
      }
      const get = (r: number, c: number) => page.rows[r]?.[c] ?? null;
      const spec = await openSeries(ui, { seed: get(range.y, range.x), rows: range.height, cols: range.width });
      if (spec) await writeFill(fillSeries(get, range, spec));
    });
  const fillDown = () => fillRange("down");
  const fillRight = () => fillRange("right");

  /** The fill handle was dragged: continue each column's (or row's) series. */
  const fillHandle = (source: Rect, dest: Rect) => {
    if (!page) return;
    void writeFill(fillFromHandle((r, c) => page.rows[r]?.[c] ?? null, source, dest));
  };

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
      // Writing a workbook over itself: values only, once per file a session.
      if (tab.sheet !== null && (path === null || path === tab.path) && !workbookConfirmed.current.has(tab.path)) {
        const name = tab.path.split(/[\\/]/).pop() ?? tab.path;
        const choice = await ui.ask(t("sheets.save_title"), t("sheets.save_msg", { file: name }), [
          { id: "cancel", label: t("btn.cancel") },
          { id: "save_as", label: t("sheets.save_as_copy") },
          { id: "save", label: t("btn.save"), primary: true },
        ]);
        if (choice === "save_as") return saveAsRef.current(tab);
        if (choice !== "save") return false;
        workbookConfirmed.current.add(tab.path);
      }
      setBusy(true);
      status(t("status.saving"));
      try {
        const s = await api.saveTab(tab.id, path);
        upsertTab(s);
        // Other tabs of a workbook show its sheets under their saved names.
        if (tab.sheet !== null) (await api.tabs()).forEach(upsertTab);
        const st = tabUi.current.get(tab.id);
        if (st) st.sort = null;
        if (tab.id === active) await loadPage(tab.id, 1);
        else setBusy(false);
        void refreshSchema();
        // Files have no place for formulas: say that they were saved as values.
        const done = t("success.save_complete_msg", { path: s.path });
        await ui.alert(t("success.save_complete"), tab.formulas.length ? `${done}\n\n${t("success.formulas_as_values", { count: tab.formulas.length })}` : done);
        return true;
      } catch (e) {
        setBusy(false);
        await ui.alert(t("error.save_failed"), t("error.save_failed_msg", { error: errorMessage(e) }), "error");
        return false;
      }
    },
    [status, t, upsertTab, active, loadPage, refreshSchema, ui],
  );

  /** Where a result tab is saved by default: next to its source file (the
   * app's working folder is no place for it). */
  const resultSavePath = (tab: TabSummary): string => {
    const source = tabs.find((x) => x.id === tab.origin?.source_id)?.path ?? "";
    const dir = source.slice(0, Math.max(source.lastIndexOf("/"), source.lastIndexOf("\\")) + 1);
    return `${dir}${tab.name}.parquet`;
  };

  const saveAs = useCallback(
    async (tab: TabSummary): Promise<boolean> => {
      const path = await saveDialog({
        title: t("dialog.save_as"),
        defaultPath: tab.path || resultSavePath(tab),
        filters: [
          { name: "Parquet", extensions: ["parquet"] },
          { name: "CSV", extensions: ["csv", "tsv"] },
          { name: "JSON", extensions: ["json", "jsonl"] },
          { name: "Excel", extensions: ["xlsx"] },
        ],
      });
      return path ? saveTab(tab, path) : false;
    },
    [saveTab, t, tabs],
  );
  saveAsRef.current = saveAs;

  const save = useCallback(async () => {
    if (!activeTab) return;
    // A result tab is not a file yet.
    if (activeTab.origin) {
      await saveAs(activeTab);
      return;
    }
    if (!activeTab.steps.length && !activeTab.pending_edits && !activeTab.sheets_changed) return;
    await saveTab(activeTab, null);
  }, [activeTab, saveTab, saveAs]);

  /** Ask about unsaved edits of `tab`. True when it may be closed. */
  const resolveUnsaved = useCallback(
    async (tab: TabSummary, closingTab = false): Promise<boolean> => {
      // Sheet changes belong to the workbook: they go with its last tab.
      const lastOfBook = tab.sheets_changed && !tabs.some((x) => x.id !== tab.id && x.path === tab.path);
      if (lastOfBook) {
        if (tab.id !== active) switchTo(tab.id);
        const choice = await ui.ask(t("warning.unsaved_changes"), t("sheets.unsaved_close", { count: tab.pending_edits }), [
          { id: "discard", label: t("dialog.unsaved.discard"), danger: true },
          { id: "cancel", label: t("btn.cancel") },
          { id: "save", label: t("btn.save"), primary: true },
        ]);
        if (choice === "save") return saveTab(tab, null);
        return choice === "discard";
      }
      if (tab.origin) {
        if (tab.id !== active) switchTo(tab.id);
        const choice = await ui.ask(t("table.unsaved_title"), t("table.unsaved_msg", { name: tab.name }), [
          { id: "discard", label: t("dialog.unsaved.discard"), danger: true },
          { id: "cancel", label: t("btn.cancel") },
          { id: "save", label: t("table.save_as"), primary: true },
        ]);
        if (choice === "save") return saveAsRef.current(tab);
        return choice === "discard";
      }
      // Closing one tab with steps (filters, operations) but no cell edits:
      // the window asks about these once for all tabs when it closes.
      if (closingTab && !tab.pending_edits && tab.steps.length && settings.warn_on_exit_with_transforms) {
        if (tab.id !== active) switchTo(tab.id);
        const choice = await ui.ask(t("dialog.close_tab.title", { name: tab.name }), t("dialog.close_tab.steps", { count: tab.steps.length }), [
          { id: "discard", label: t("dialog.unsaved.discard"), danger: true },
          { id: "cancel", label: t("btn.cancel") },
          { id: "save", label: t("btn.save"), primary: true },
        ]);
        if (choice === "save") return saveTab(tab, null);
        return choice === "discard";
      }
      if (!tab.pending_edits) return true;
      if (tab.id !== active) switchTo(tab.id);
      const choice = await openUnsavedChanges(ui, { count: tab.pending_edits });
      if (choice === "cancel") return false;
      if (choice === "save") return saveTab(tab, null);
      return true;
    },
    [active, switchTo, ui, saveTab, tabs, t, settings.warn_on_exit_with_transforms],
  );

  const closeTab = useCallback(
    async (id: number) => {
      const tab = tabs.find((x) => x.id === id);
      // The tab in front before an unsaved-changes prompt switched to `id`.
      const before = activeRef.current;
      if (!tab || !(await resolveUnsaved(tab, true))) return;
      try {
        await api.closeTab(id);
      } catch {
        /* already gone */
      }
      tabUi.current.delete(id);
      const rest = tabs.filter((x) => x.id !== id);
      setTabs(rest);
      // The prompt about unsaved changes switches to the tab it asks about.
      if (id === active || id === activeRef.current) {
        if (before !== null && before !== id && rest.some((x) => x.id === before)) {
          switchTo(before);
        } else if (rest.length) {
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
  // The settings hold the theme and language too: Settings shows them.
  const onTheme = useCallback((info: { name: string; css: string }) => {
    applyThemeCss(info.css);
    setThemeKey(info.name + Date.now());
    setSettings((s) => ({ ...s, current_theme: info.name }));
  }, []);
  /** Saved settings: the page size may have changed, so tabs and the page reload. */
  const onSettingsSaved = useCallback(
    (s: Settings) => {
      setSettings(s);
      void api.tabs().then((list) => list.forEach(upsertTab)).catch(() => undefined);
      if (active !== null) void loadPage(active, 1);
    },
    [active, upsertTab, loadPage],
  );
  const onLanguage = useCallback(
    (lang: string, tr: Record<string, string>) => {
      setTranslations(tr);
      setSettings((s) => ({ ...s, current_language: lang }));
      // The status line ("Showing page 1 of 5") is in the old language
      // until the page is read again.
      const id = activeRef.current;
      if (id !== null) void loadPage(id, tabUi.current.get(id)?.page ?? 1);
    },
    [loadPage],
  );

  // ------------------------------------------------------------ menus
  // ------------------------------------------------------------ AI assistant
  const loadAiProviders = useCallback(async () => {
    try {
      const info = await api.aiProviders();
      setAiProviders(info);
      setAiProvider((cur) =>
        cur && info.providers.some((p) => p.config.id === cur)
          ? cur
          : (info.providers.find((p) => p.is_default) ?? info.providers[0])?.config.id ?? null,
      );
    } catch {
      setAiProviders({ providers: [], keychain: false });
    }
  }, []);
  const toggleAi = useCallback(() => setView({ show_ai_panel: !aiOpen }), [setView, aiOpen]);
  const manageProviders = async () => {
    await openAiProviders(ui);
    await loadAiProviders();
  };
  useEffect(() => {
    if (aiOpen) void loadAiProviders();
  }, [aiOpen, loadAiProviders]);
  // The start screen says whether an AI model is set up.
  useEffect(() => {
    if (!activeTab && aiProviders === null) void loadAiProviders();
  }, [activeTab, aiProviders, loadAiProviders]);

  const menus = [
    {
      title: t("menu.file"),
      icon: "file" as const,
      items: (): MenuEntry[] => [
        { label: t("menu.file.new_window"), icon: "newWindow", shortcut: "Ctrl+N", onSelect: () => void api.newWindow() },
        { type: "separator" },
        { label: t("menu.file.open"), icon: "open", shortcut: "Ctrl+O", onSelect: () => void browse() },
        { label: t("menu.file.open_options"), icon: "openOptions", onSelect: () => void browseWithOptions() },
        {
          label: t("menu.file.recent_files"),
          icon: "recent",
          submenu: recents.length
            ? [
                ...recents.map((r) => ({ label: r, onSelect: () => void openPath(r) })),
                { type: "separator" as const },
                { label: t("menu.file.clear_recents"), onSelect: () => void api.clearRecents() },
              ]
            : [{ label: t("menu.file.no_recents"), disabled: true }],
        },
        { type: "separator" },
        { label: t("menu.file.save"), icon: "save", shortcut: "Ctrl+S", disabled: !activeTab, onSelect: () => void save() },
        { label: t("menu.file.save_as"), icon: "saveAs", shortcut: "Ctrl+Shift+S", disabled: !activeTab, onSelect: () => activeTab && void saveAs(activeTab) },
        { label: t("menu.file.export"), icon: "export", shortcut: "Ctrl+E", disabled: !activeTab, onSelect: () => void exportTab() },
        { type: "separator" },
        { label: t("menu.file.close_tab"), icon: "close", shortcut: "Ctrl+W", disabled: !activeTab, onSelect: () => activeTab && void closeTab(activeTab.id) },
        { label: t("menu.file.settings"), icon: "settings", onSelect: () => void openSettingsDialog(ui, { onSettings: onSettingsSaved, onTheme, onLanguage }) },
        { type: "separator" },
        {
          label: t("menu.file.exit"),
          icon: "exit",
          onSelect: () => {
            exiting.current = true;
            void getCurrentWindow().close();
          },
        },
      ],
    },
    {
      title: t("menu.edit"),
      icon: "editMenu" as const,
      items: (): MenuEntry[] => [
        { label: t("menu.edit.undo"), icon: "undo", shortcut: "Ctrl+Z", disabled: !activeTab?.steps.length, onSelect: () => void undo() },
        { label: t("menu.edit.reset"), icon: "reset", disabled: !activeTab, onSelect: () => void resetTab() },
        { type: "separator" },
        { label: t("menu.edit.find"), icon: "search", shortcut: "Ctrl+F", disabled: !activeTab, onSelect: openSearch },
        { label: t("menu.edit.find_next"), icon: "arrowDown", shortcut: "F3", disabled: !activeTab, onSelect: () => (search ? searchKeys.current?.next() : openSearch()) },
        { label: t("menu.edit.find_prev"), icon: "arrowUp", shortcut: "Shift+F3", disabled: !activeTab, onSelect: () => (search ? searchKeys.current?.prev() : openSearch()) },
        { type: "separator" },
        { label: t("context.fill"), icon: "fill", disabled: !activeTab || !page?.editable, submenu: fillMenu() },
      ],
    },
    {
      title: t("menu.view"),
      icon: "view" as const,
      items: (): MenuEntry[] => [
        { label: t("menu.view.editor"), icon: "database", checked: editorOpen, onSelect: () => setView({ show_query_panel: !editorOpen }) },
        {
          label: t("menu.view.editor_collapsed"),
          icon: "collapse",
          checked: editorCollapsed,
          disabled: !editorOpen,
          onSelect: () => setView({ query_panel_collapsed: !editorCollapsed }),
        },
        { label: t("menu.view.steps"), icon: "steps", checked: stepsOpen, onSelect: () => setView({ show_steps_panel: !stepsOpen }) },
        { type: "separator" },
        { label: t("menu.view.table_info"), icon: "info", disabled: !activeTab, onSelect: () => void showTableInfo() },
        { label: t("menu.view.chart"), icon: "chart", shortcut: "Ctrl+Shift+C", disabled: !activeTab, onSelect: () => void showChart() },
        { label: t("menu.view.conditional_format"), icon: "format", disabled: !activeTab, onSelect: () => void editFormats() },
        {
          label: t("menu.view.unfreeze"),
          icon: "unpin",
          disabled: !(activeTab && (tabUi.current.get(activeTab.id)?.frozen ?? 0) > 0),
          onSelect: () => activeTab && updateView(activeTab.id, { frozen: 0 }),
        },
        { type: "separator" },
        { label: t("menu.view.theme"), icon: "theme", onSelect: () => void openThemeSelector(ui, { onTheme }) },
        { label: t("menu.view.language"), icon: "language", onSelect: () => void openLanguageSelector(ui, { onLanguage }) },
      ],
    },
    {
      title: t("menu.rows"),
      icon: "rows" as const,
      items: (): MenuEntry[] => [
        { label: t("menu.data.filter"), icon: "filter", shortcut: "Ctrl+Shift+L", disabled: !activeTab, onSelect: () => void runOpDialog(openFilter) },
        { label: t("menu.data.sort"), icon: "sort", disabled: !activeTab, onSelect: () => void runOpDialog(openSort) },
        { label: t("menu.data.slice"), icon: "rows", disabled: !activeTab, onSelect: () => void runSlice() },
        { type: "separator" },
        { label: t("menu.operations.drop_duplicates"), icon: "dropDuplicates", disabled: !activeTab, onSelect: () => void runDropDuplicates() },
        { label: t("menu.operations.mark_duplicates"), icon: "markDuplicates", disabled: !activeTab, onSelect: () => void runDropDuplicates(undefined, true) },
        { label: t("menu.operations.drop_null"), icon: "dropNull", disabled: !activeTab, onSelect: () => void runDropNull() },
        ...(operationsFor(customDefs, false).length
          ? [{ type: "separator" as const }, ...customEntries(operationsFor(customDefs, false))]
          : []),
      ],
    },
    {
      title: t("menu.column"),
      icon: "column" as const,
      items: (): MenuEntry[] => {
        // The column of the selected cell.
        const column = currentCell ? page?.columns[currentCell.col]?.name : undefined;
        const head: MenuEntry[] = [
          { label: column ? t("menu.column.selected", { column }) : t("menu.column.none"), disabled: true },
          { type: "separator" },
        ];
        const all: MenuEntry[] = [
          { type: "separator" },
          { label: t("menu.data.columns"), icon: "column", disabled: !activeTab, onSelect: () => void runOpDialog(openColumns) },
        ];
        if (!activeTab) return [...head, ...all];
        if (!column) {
          return [
            ...head,
            { label: t("formula.new_end"), icon: "formula", onSelect: () => startFormula(null) },
            ...all,
          ];
        }
        return [...head, ...columnMenu(column), ...all];
      },
    },
    {
      title: t("menu.table"),
      icon: "table" as const,
      items: (): MenuEntry[] => [
        { label: t("menu.reshape.group_by"), icon: "groupBy", shortcut: "Ctrl+G", disabled: !activeTab, onSelect: () => void runOpDialog(openGroupByCustom, undefined, true) },
        { label: t("menu.reshape.pivot"), icon: "pivot", disabled: !activeTab, onSelect: () => void runOpDialog(openPivot, undefined, true) },
        { label: t("menu.reshape.melt"), icon: "melt", disabled: !activeTab, onSelect: () => void runOpDialog(openMelt) },
        { label: t("menu.reshape.transpose"), icon: "transpose", disabled: !activeTab, onSelect: () => void runTranspose() },
        { type: "separator" },
        { label: t("menu.reshape.value_counts"), icon: "valueCounts", disabled: !activeTab, onSelect: () => void runOpDialog(openValueCounts, undefined, true) },
        { label: t("menu.reshape.describe"), icon: "describe", disabled: !activeTab, onSelect: () => void runDescribe() },
        { type: "separator" },
        { label: t("menu.operations.join"), icon: "join", disabled: !activeTab, onSelect: () => void runJoin() },
        { label: t("menu.operations.append"), icon: "append", disabled: !activeTab, onSelect: () => void runAppend() },
        ...(operationsFor(customDefs, true).length
          ? [{ type: "separator" as const }, ...customEntries(operationsFor(customDefs, true))]
          : []),
        { type: "separator" },
        { label: t("custom.menu.new_operation"), icon: "plus", onSelect: () => void newCustom("operation") },
        { label: t("custom.menu.from_steps"), icon: "steps", disabled: !activeTab?.steps.length, onSelect: () => void saveStepsAsOperation() },
        { label: t("custom.menu.manage"), icon: "custom", onSelect: manageCustom },
      ],
    },
    {
      title: t("menu.ai"),
      icon: "ai" as const,
      items: (): MenuEntry[] => [
        { label: t("menu.ai.assistant"), icon: "ai", shortcut: "Ctrl+Shift+A", checked: aiOpen, onSelect: () => toggleAi() },
        { label: t("menu.ai.providers"), icon: "key", onSelect: () => void manageProviders() },
        { type: "separator" },
        { label: t("menu.ai.connect"), icon: "connect", onSelect: () => void openConnectAi(ui) },
      ],
    },
    {
      title: t("menu.help"),
      icon: "help" as const,
      items: (): MenuEntry[] => [
        { label: t("menu.help.expression"), icon: "book", onSelect: () => void openExpressionHelp(ui) },
        { label: t("menu.help.shortcuts"), icon: "keyboard", shortcut: "Ctrl+/", onSelect: () => void openShortcutsDialog(ui) },
        { type: "separator" },
        { label: t("menu.help.about"), icon: "info", onSelect: () => void openAboutDialog(ui) },
      ],
    },
  ];

  // ------------------------------------------------------------ effects
  // Keep refs to the latest handlers for global listeners.
  const filterRows = () => void runOpDialog(openFilter);
  const groupBy = () => void runOpDialog(openGroupByCustom, undefined, true);
  /** An AI client changed a tab (after the user approved it): show the result. */
  const refreshFromAi = async (id: number) => {
    try {
      const s = await api.tabSummary(id);
      if (!tabUi.current.has(id)) {
        // A table operation by the AI opened a new tab.
        tabUi.current.set(id, newTabUi(s.name));
        upsertTab(s);
        switchTo(id);
        void refreshSchema();
        status(t("table.opened", { name: s.name, source: s.origin?.source ?? "" }), 6000);
        return;
      }
      upsertTab(s);
      if (id === active) await loadPage(id, 1);
      const last = s.steps[s.steps.length - 1];
      status(last ? t("mcp.tab_updated", { step: stepText(t, last) }) : t("mcp.tab_updated_plain"), 5000);
      void refreshSchema();
    } catch {
      /* the tab may have been closed meanwhile */
    }
  };
  /** A custom tool changed: formulas follow the current definitions, and
   * one whose function was deleted or broke now shows its error. */
  const refreshFormulasAfterCustom = async () => {
    try {
      const list = await api.tabs();
      const before = new Map(tabs.map((x) => [x.id, JSON.stringify(x.formulas)]));
      let broken = 0;
      for (const s of list) {
        if (before.get(s.id) === JSON.stringify(s.formulas)) continue;
        upsertTab(s);
        broken += s.formulas.filter((f) => f.error).length;
        if (s.id === active) await loadPage(s.id, page?.page ?? 1);
      }
      if (broken) status(t("status.formulas_broken", { count: broken }), 10000);
    } catch {
      /* the tabs reload on their next change */
    }
  };
  const handlers = useRef({ browse, save, saveAs, exportTab, undo, openSearch, closeTab, openPath, activeTab, search, dialogs, filterRows, groupBy, fillDown, fillRight, showChart, refreshFromAi, toggleAi, refreshFormulasAfterCustom, reviewCustomOffer });
  handlers.current = { browse, save, saveAs, exportTab, undo, openSearch, closeTab, openPath, activeTab, search, dialogs, filterRows, groupBy, fillDown, fillRight, showChart, refreshFromAi, toggleAi, refreshFormulasAfterCustom, reviewCustomOffer };

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
      else if (ctrl && e.shiftKey && k === "a") run(() => h.toggleAi());
      else if (ctrl && !e.shiftKey && k === "d" && !typing) run(() => h.activeTab && void h.fillDown());
      // Ctrl+R fills right, as in a spreadsheet (and never reloads the window).
      else if (ctrl && !e.shiftKey && k === "r" && !typing) run(() => h.activeTab && void h.fillRight());
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
      // AI clients (parvu mcp --attach): approval cards, refreshes, client count.
      getCurrentWindow().listen<McpProposal>("mcp-proposal", async (e) => {
        const approved = await openMcpProposal(ui, e.payload);
        const live = await api.mcpDecide(e.payload.id, approved).catch(() => false);
        if (!live) status(t("mcp.proposal.expired"), 5000);
      }),
      getCurrentWindow().listen<number>("mcp-changed", (e) => void handlers.current.refreshFromAi(e.payload)),
      getCurrentWindow().listen<CustomOffer>("custom-proposal", (e) => void handlers.current.reviewCustomOffer(e.payload)),
      // Files from another `parvu` start (file manager, command line): one ParVu runs per profile.
      getCurrentWindow().listen<string[]>("open-files", async (e) => {
        for (const p of e.payload) await handlers.current.openPath(p);
      }),
      // Built-in assistant: streamed events and the end of a run, per tab.
      getCurrentWindow().listen<AiRunEvent>("ai-event", (e) =>
        updateChat(e.payload.tab_id ?? 0, (c) => (c.runId === e.payload.run_id ? applyEvent(c, e.payload.event) : c)),
      ),
      getCurrentWindow().listen<AiRunFinished>("ai-finished", (e) =>
        updateChat(e.payload.tab_id ?? 0, (c) =>
          c.runId === e.payload.run_id ? finishRun(c, e.payload.error, e.payload.cancelled) : c,
        ),
      ),
      listen<number>("mcp-clients", (e) => setAiClients(e.payload)),
      // Custom tools changed (any window), or some need attention after an update.
      listen("custom-changed", () => {
        reloadCustom();
        void handlers.current.refreshFormulasAfterCustom();
      }),

      // The memory guard stopped running queries (memguard.rs).
      listen<number>("memory-guard", (e) => status(t("memory.guard_stopped", { mb: e.payload }), 10000)),
      getCurrentWebview().onDragDropEvent(async (e) => {
        if (e.payload.type !== "drop") return;
        for (const p of e.payload.paths) {
          const ext = "." + (p.split(".").pop() ?? "").toLowerCase();
          if (boot.extensions.includes(ext)) await handlers.current.openPath(p);
          else status(t("error.unsupported_drop", { path: p }), 5000);
        }
      }),
    ];
    void api.mcpStatus().then((s) => setAiClients(s.clients)).catch(() => undefined);
    return () => unsubs.forEach((u) => void u.then((f) => f()));
  }, [boot.extensions, status, t, ui, updateChat]);

  // Closing the window: with AI clients on, the last window may hide in the
  // tray instead (tray.rs `close_action`). Otherwise unsaved edits, then
  // (optionally) the transforms warning. File → Exit always closes.
  const closeState = useRef({ tabs, settings, resolveUnsaved, exportTab, ui });
  closeState.current = { tabs, settings, resolveUnsaved, exportTab, ui };
  const exiting = useRef(false);
  useEffect(() => {
    const win = getCurrentWindow();
    // Ask about anything closing would lose; false when the user cancels.
    const confirmClose = async (): Promise<boolean> => {
      const c = closeState.current;
      for (const tab of c.tabs) if (!(await c.resolveUnsaved(tab))) return false;
      const withSteps = c.tabs.filter((x) => x.steps.length && !x.pending_edits);
      if (c.settings.warn_on_exit_with_transforms && withSteps.length) {
        const r = await openConfirmClose(c.ui, { tabs: withSteps.map((x) => x.name) });
        if (r.result === "cancel") return false;
        if (r.result === "save_and_close" && !(await c.exportTab())) return false;
        if (r.dontAsk) {
          const s = await api.updateSettings({ warn_on_exit_with_transforms: false });
          setSettings(s);
          await c.ui.alert(t("dialog.confirm_close.setting_saved"), t("dialog.confirm_close.setting_saved_msg"));
        }
      }
      return true;
    };
    const un = win.onCloseRequested(async (event) => {
      event.preventDefault();
      const c = closeState.current;
      const forced = exiting.current;
      exiting.current = false;
      if (!forced) {
        const dirty = windowIsDirty(c.tabs, c.settings.warn_on_exit_with_transforms);
        const decision = await api.windowCloseAction(dirty).catch(() => null);
        if (decision?.action === "hide") {
          if (decision.notice) await c.ui.alert(t("tray.hidden_title"), t("tray.hidden_message"));
          await api.hideToTray();
          return;
        }
      }
      if (!(await confirmClose())) return;
      await win.destroy();
    });
    // "Quit ParVu" in the tray (or `parvu --quit`) found something to ask about here.
    const unQuit = win.listen("quit-requested", async () => {
      if (await confirmClose()) await win.destroy();
    });
    return () => {
      void un.then((f) => f());
      void unQuit.then((f) => f());
    };
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
  /** The AI panel's buttons on the assistant's SQL and results. */
  const aiActions: AiActions = {
    tabs,
    // The SQL runs again with the assistant's meaning of table names (each
    // tab's current result), checked by the same read-only guard.
    openSql: activeTab ? (sql) => void makeTable(activeTab, { kind: "tabs_sql", sql }) : null,
    editSql: activeTab
      ? (sql) => {
          setMode("sql");
          setEditorText(sql);
          if (st) {
            st.mode = "sql";
            st.query = sql;
          }
          if (!editorOpen || editorCollapsed) setView({ show_query_panel: true, query_panel_collapsed: false });
          status(t("ai.action.sql_in_editor"), 5000);
        }
      : null,
  };

  // Colour scales need each column's min and max over the whole result;
  // re-read them when the data or the scaled columns change.
  const scaleColumns = [...new Set((st?.formats ?? []).filter((f) => f.kind === "scale").map((f) => f.column))];
  const rangeKey = activeTab ? `${activeTab.id}|${activeTab.current_query}|${activeTab.pending_edits}|${scaleColumns.join("\u0000")}` : "";
  useEffect(() => {
    if (!activeTab || !scaleColumns.length) return;
    const id = activeTab.id;
    let cancelled = false;
    // One scan for every scaled column, stopped when the data changes again.
    const columns = scaleColumns;
    const job = api.columnRanges(id, columns, { label: t("job.ranges"), total: activeTab.total_rows });
    job.result.then(
      (found) => {
        if (cancelled) return;
        const ranges: Record<string, { min: number; max: number }> = {};
        columns.forEach((c, i) => {
          const r = found[i];
          if (r) ranges[c] = { min: r[0], max: r[1] };
        });
        updateView(id, { ranges });
      },
      () => undefined,
    );
    return () => {
      cancelled = true;
      job.cancel();
    };
    // rangeKey covers the tab, its data and the scaled columns.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rangeKey]);

  const sqlFont = String(settings.default_sql_font || "monospace");
  const sqlSize = num(settings.default_sql_font_size, 11);
  const headerLabel = activeTab
    ? t(mode === "sql" ? "label.sql_query" : "label.expression_query", { table_name: activeTab.name })
    : "";

  /** Quick operations next to the query editor (on its title row when collapsed). */
  const opsButtons = (
    <>
      <button className="text" onClick={() => void showTableInfo()}>
        <Icon name="info" />
        {t("btn.table_info")}
      </button>
      <span className="divider" />
      <button className="text" onClick={() => void runOpDialog(openFilter)}>
        <Icon name="filter" />
        {t("btn.filter")}
      </button>
      <button className="text" onClick={() => void runOpDialog(openGroupByCustom, undefined, true)}>
        <Icon name="groupBy" />
        {t("btn.group_by")}
      </button>
      <button className="text" onClick={() => void showChart()}>
        <Icon name="chart" />
        {t("btn.chart")}
      </button>
      <span className="divider" />
      <button className="text" onClick={() => startFormula()}>
        <Icon name="formula" />
        {t("btn.formula")}
      </button>
      <button className="text" onClick={() => void runJoin()}>
        <Icon name="join" />
        {t("menu.operations.join")}
      </button>
      <button className="text" onClick={() => void runAppend()}>
        <Icon name="append" />
        {t("menu.operations.append")}
      </button>
    </>
  );

  const startResize = (e: React.MouseEvent) => {
    e.preventDefault();
    const startY = e.clientY;
    const startH = editorHeight;
    let h = startH;
    const move = (ev: MouseEvent) => {
      h = Math.max(80, Math.min(window.innerHeight - 250, startH + ev.clientY - startY));
      setEditorHeight(h);
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      if (h !== startH) setView({ query_panel_height: Math.round(h) });
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  return (
    <UiContext.Provider value={ui}>
      <div className="app">
        <MenuBar menus={menus} />
        <div className="app-body">
        {!activeTab ? (
          <Welcome
            t={t}
            recents={recents}
            onOpen={(p) => void openPath(p)}
            onBrowse={() => void browse()}
            onBrowseWithOptions={() => void browseWithOptions()}
            aiReady={(aiProviders?.providers.length ?? 0) > 0}
            onAiAssistant={() => (aiProviders?.providers.length ? (aiOpen ? undefined : toggleAi()) : void manageProviders())}
            onConnectAi={() => void openConnectAi(ui)}
            onShortcuts={() => void openShortcutsDialog(ui)}
            onGuide={() => void openExpressionHelp(ui)}
            onChanges={() => void openUrl(`https://github.com/AzizNadirov/ParVu/releases/tag/v${boot.version}`).catch(() => undefined)}
            themeName={settings.current_theme || boot.theme.name}
            languageName={boot.languages.find((l) => l.code === (settings.current_language || boot.language))?.native_name ?? ""}
            onTheme={() => void openThemeSelector(ui, { onTheme })}
            onLanguage={() => void openLanguageSelector(ui, { onLanguage })}
          />
        ) : (
          <main className="workspace">
            {editorOpen && (
              <section className={`editor-panel${editorCollapsed ? " collapsed" : ""}`} style={editorCollapsed ? undefined : { height: editorHeight }}>
                <div className="editor-toolbar row">
                  <button
                    className="icon panel-toggle"
                    aria-expanded={!editorCollapsed}
                    aria-label={t(editorCollapsed ? "editor.expand" : "editor.collapse")}
                    title={t(editorCollapsed ? "editor.expand" : "editor.collapse")}
                    onClick={() => setView({ query_panel_collapsed: !editorCollapsed })}
                  >
                    <Icon name={editorCollapsed ? "chevronRight" : "chevronDown"} size={18} />
                  </button>
                  <strong className="ellipsis">{headerLabel}</strong>
                  {activeTab.sheet !== null && (
                    <button
                      className={`text small-btn sheet-chip${activeTab.sheets_changed ? " changed" : ""}`}
                      aria-label={t("sheets.menu_title")}
                      aria-haspopup="menu"
                      onClick={(e) => void showSheetMenu(e.currentTarget)}
                    >
                      {t("sheets.chip", { sheet: activeTab.sheet })}
                      <Icon name="chevronDown" size={16} />
                    </button>
                  )}
                  {activeTab.origin && (
                    <span className="origin-chip">
                      <button
                        className="text small-btn"
                        disabled={!tabs.some((x) => x.id === activeTab.origin?.source_id)}
                        aria-label={t("table.go_to_source", { source: activeTab.origin.source })}
                        title={t("table.from", { source: activeTab.origin.source, what: stepText(t, activeTab.origin.label) })}
                        onClick={() => activeTab.origin && switchTo(activeTab.origin.source_id)}
                      >
                        <span className="ellipsis">
                          {t("table.from", { source: activeTab.origin.source, what: stepText(t, activeTab.origin.label) })}
                        </span>
                      </button>
                      <button
                        className="icon"
                        disabled={!tabs.some((x) => x.id === activeTab.origin?.source_id)}
                        aria-label={t("table.refresh")}
                        title={
                          tabs.some((x) => x.id === activeTab.origin?.source_id)
                            ? t("table.refresh")
                            : t("table.source_closed", { source: activeTab.origin.source })
                        }
                        onClick={() => void refreshResult(activeTab)}
                      >
                        <Icon name="reset" size={16} />
                      </button>
                    </span>
                  )}
                  <span className="spacer" />
                  {editorCollapsed ? (
                    opsButtons
                  ) : (
                    <>
                  <span className="muted">{t("editor.mode")}</span>
                  <div className="segmented">
                    <button
                      className={mode === "sql" ? "active" : ""}
                      onClick={() => {
                        setMode("sql");
                        if (st) st.mode = "sql";
                      }}
                    >
                      {mode === "sql" && <Icon name="check" size={16} />}
                      {t("editor.mode.sql")}
                    </button>
                    <button
                      className={mode === "expression" ? "active" : ""}
                      onClick={() => {
                        setMode("expression");
                        if (st) st.mode = "expression";
                      }}
                    >
                      {mode === "expression" && <Icon name="check" size={16} />}
                      {t("editor.mode.expression")}
                    </button>
                  </div>
                    </>
                  )}
                </div>
                {!editorCollapsed && (
                  <>
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
                  sqlFunctions={sqlFunctions}
                  fontFamily={sqlFont}
                  fontSize={sqlSize}
                />
                {mode === "expression" && <div className="expr-preview mono ellipsis" title={preview}>{preview}</div>}
                  </>
                )}
                {!editorCollapsed && (
                <div className="editor-actions row">
                  {!editorCollapsed && (
                    <button className="primary" disabled={busy} onClick={() => void execute()} title={t("editor.run_hint")}>
                      <Icon name="run" />
                      {t("btn.execute")}
                    </button>
                  )}
                  {!editorCollapsed && (
                    <button className="outlined" disabled={busy} onClick={() => void resetTab()}>
                      <Icon name="reset" />
                      {t("btn.reset")}
                    </button>
                  )}
                  {mode === "sql" && !editorCollapsed && (
                    <label className="check small">
                      <input type="checkbox" checked={sqlToNewTab} onChange={(e) => setSqlToNewTab(e.target.checked)} />
                      {t("table.sql_new_tab")}
                    </label>
                  )}
                  {!editorCollapsed && <span className="muted small">{t("editor.run_hint")}</span>}
                  <span className="spacer" />
                  {opsButtons}
                </div>
                )}
                {!editorCollapsed && <div className="resize-handle" onMouseDown={startResize} />}
              </section>
            )}
            <StepsPanel
              t={t}
              steps={activeTab.steps}
              open={stepsOpen}
              onToggle={() => setView({ show_steps_panel: !stepsOpen })}
              onUndo={() => void undo()}
              onSaveAsOperation={() => void saveStepsAsOperation()}
              busy={busy}
            />
            {formulaEdit && (
              <FormulaBar
                ref={formulaBar}
                t={t}
                schema={formulaSchema}
                dark={darkTheme}
                editing
                target={formulaEdit.name}
                isNew={formulaEdit.isNew}
                onNameChange={(name) => setFormulaEdit({ ...formulaEdit, name })}
                value={formulaEdit.source}
                error={formulaEdit.error}
                errorAt={formulaEdit.errorAt ?? null}
                onChange={(source) => setFormulaEdit({ ...formulaEdit, source, error: "" })}
                onApply={() => void applyFormula()}
                onCancel={() => setFormulaEdit(null)}
              />
            )}
            <section className="grid-panel">
              <DataGrid
                ref={grid}
                page={page}
                sort={st?.sort ?? null}
                themeKey={themeKey}
                nullText={t("grid.null")}
                select={gridSelect}
                pickingColumns={!!formulaEdit}
                onHeaderClick={(c, shift) =>
                  formulaEdit
                    ? shift
                      ? formulaBar.current?.extendRange(c)
                      : formulaBar.current?.insert(refFor(c))
                    : (setCurrentCell((cur) => ({ row: cur?.row ?? 0, col: Math.max(0, page?.columns.findIndex((x) => x.name === c) ?? 0) })),
                      void sortBy(c))
                }
                onHeaderMenu={(c, x, y) => setMenu({ x, y, items: headerMenu(c) })}
                onCellMenu={(r, c, x, y) => setMenu({ x, y, items: cellMenu(r, c) })}
                onFill={fillHandle}
                onNewColumn={formulaEdit ? undefined : () => startFormula(null)}
                newColumnLabel={t("formula.new_end")}
                onEdit={(r, c, v) => void editCell(r, c, v)}
                frozen={st?.frozen ?? 0}
                widths={st?.widths ?? NO_WIDTHS}
                onResize={(c, w) => activeTab && updateView(activeTab.id, { widths: { ...(st?.widths ?? {}), [c]: w } })}
                formats={st?.formats ?? NO_FORMATS}
                ranges={st?.ranges ?? NO_RANGES}
                sourceColumns={sourceCols}
                formulaColumns={formulaNames}
                brokenFormulas={brokenFormulas}
                highlights={formulaHighlights}
                onCurrentCell={(row, col) => setCurrentCell({ row, col })}
                onFormulaCell={editFormula}
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
        {aiOpen && (
          <AiPanel
            ui={ui}
            tabId={activeTab?.id ?? null}
            tabName={activeTab?.name ?? null}
            chat={chats[activeTab?.id ?? 0] ?? emptyChat()}
            onStart={(q, runId) => updateChat(activeTab?.id ?? 0, (c) => startRun(c, q, runId))}
            onClear={() => {
              const id = activeTab?.id ?? null;
              void api.aiClearChat(id);
              updateChat(id ?? 0, () => emptyChat());
            }}
            providers={aiProviders}
            providerId={aiProvider}
            onProvider={setAiProvider}
            onOpenProviders={() => void manageProviders()}
            onConnectClient={() => void openConnectAi(ui)}
            onClose={toggleAi}
            actions={aiActions}
          />
        )}
        </div>
        {tabs.length > 0 && (
          <TabBar t={t} tabs={tabs} active={active} onSelect={switchTo} onClose={(id) => void closeTab(id)} onAdd={() => void browse()} />
        )}
        <footer className="status-bar row">
          <span className="ellipsis grow">{statusText}</span>
          {activeTab && !activeTab.is_base_query && <span className="muted">{t("status.read_only")}</span>}
          {activeTab && activeTab.pending_edits > 0 && (
            <span className="warn-text">{t("status.unsaved_edits", { count: activeTab.pending_edits })}</span>
          )}
          {activeTab?.sheets_changed && <span className="warn-text">{t("sheets.status_unsaved")}</span>}
          {aiClients > 0 && (
            <button className="text small-btn pv-ai-chip" title={t("menu.ai.connect")} onClick={() => void openConnectAi(ui)}>
              <Icon name="ai" size={16} />
              {t("mcp.status.connected", { count: aiClients })}
            </button>
          )}
          <StatusJobs t={t} />
          {activeTab && <span className="nowrap">{activeTab.total_rows === 1 ? t("pagination.rows_one") : t("pagination.rows", { total_rows: activeTab.total_rows })}</span>}
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
