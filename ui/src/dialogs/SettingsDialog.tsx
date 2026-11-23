import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import type { Ui } from "../lib/ui";
import type { Settings, ThemeDetails, ThemeInfo, WarningCriteria } from "../lib/api";
import { api, errorMessage } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { LanguagePicker } from "./LanguageSelector";
import { ThemePreview } from "./ThemePreview";
import { exportThemeFlow, importThemeFlow } from "./themeActions";
import { clampInt, isTruthySetting, toInt } from "./logic";
import "./dialogs.css";

interface Callbacks {
  onSettings(s: Settings): void;
  onTheme(info: ThemeInfo): void;
  onLanguage(lang: string, translations: Record<string, string>): void;
}

type Tab = "general" | "theme" | "advanced" | "warnings";

/** Editable copy of the settings; numbers kept as text while typing. */
export interface SettingsForm {
  tableVar: string;
  rowsPerPage: string;
  maxRows: string;
  saveHistory: boolean;
  warnTransforms: boolean;
  keepInTray: boolean;
  copySample: string;
  memoryLimit: string;
  language: string;
  theme: string;
  sqlFont: string;
  sqlFontSize: string;
  tableFontSize: string;
  defaultQuery: string;
  defaultLimit: string;
  crashReporting: boolean;
  bugEmail: string;
  warningEnabled: boolean;
  criteria: WarningCriteria;
  detectTypes: boolean;
  dateOrder: Settings["date_order"];
  rowsThreshold: string;
  cellsThreshold: string;
  sizeThreshold: string;
  showQueryPanel: boolean;
  queryCollapsed: boolean;
  showSteps: boolean;
  showAi: boolean;
  rememberAiChats: boolean;
}

/** [form field, min, max] for every numeric input. */
export const NUMERIC_RANGES: [keyof SettingsForm, number, number][] = [
  ["rowsPerPage", 10, 10000],
  ["maxRows", 100, 100000],
  ["copySample", 1, 1000],
  ["memoryLimit", 256, 65536],
  ["sqlFontSize", 8, 24],
  ["tableFontSize", 7, 20],
  ["defaultLimit", 1, 10000],
  ["rowsThreshold", 1000, 100_000_000],
  ["cellsThreshold", 10_000, 1_000_000_000],
  ["sizeThreshold", 1, 100_000],
];

export function settingsToForm(s: Settings, language: string, theme: string): SettingsForm {
  return {
    tableVar: s.default_data_var_name,
    rowsPerPage: String(toInt(s.result_pagination_rows_per_page, 500)),
    maxRows: String(toInt(s.max_rows, 1000)),
    saveHistory: isTruthySetting(s.save_file_history),
    warnTransforms: !!s.warn_on_exit_with_transforms,
    keepInTray: s.keep_running_in_tray ?? true,
    copySample: String(toInt(s.copy_tuple_sample_size, 500)),
    memoryLimit: String(toInt(s.query_memory_limit_mb, 1024)),
    language: s.current_language || language,
    theme: s.current_theme || theme,
    sqlFont: s.default_sql_font,
    sqlFontSize: String(toInt(s.default_sql_font_size, 10)),
    tableFontSize: String(toInt(s.default_result_font_size, 11)),
    defaultQuery: s.default_sql_query,
    defaultLimit: String(toInt(s.default_limit, 500)),
    crashReporting: !!s.enable_crash_reporting,
    bugEmail: s.bug_report_email ?? "",
    warningEnabled: !!s.enable_large_dataset_warning,
    criteria: s.warning_criteria ?? "rows",
    detectTypes: s.detect_column_types ?? true,
    dateOrder: s.date_order ?? "auto",
    rowsThreshold: String(toInt(s.warning_threshold_rows, 1_000_000)),
    cellsThreshold: String(toInt(s.warning_threshold_cells, 10_000_000)),
    sizeThreshold: String(toInt(s.warning_threshold_filesize_mb, 100)),
    showQueryPanel: s.show_query_panel ?? true,
    queryCollapsed: s.query_panel_collapsed ?? true,
    showSteps: s.show_steps_panel ?? false,
    showAi: s.show_ai_panel ?? false,
    rememberAiChats: s.remember_ai_chats ?? true,
  };
}

export function invalidNumericFields(f: SettingsForm): (keyof SettingsForm)[] {
  return NUMERIC_RANGES.filter(([k, min, max]) => {
    const n = Number(f[k]);
    return !Number.isInteger(n) || n < min || n > max;
  }).map(([k]) => k);
}

/** Values sent to update_settings. Stringly-typed fields are stored as strings, as the Python app did. */
export function formToValues(f: SettingsForm): Partial<Settings> {
  const n = (k: keyof SettingsForm) => {
    const [, min, max] = NUMERIC_RANGES.find(([x]) => x === k)!;
    return clampInt(Number(f[k]), min, max);
  };
  return {
    default_data_var_name: f.tableVar.trim(),
    result_pagination_rows_per_page: String(n("rowsPerPage")),
    max_rows: String(n("maxRows")),
    save_file_history: f.saveHistory ? "True" : "False",
    default_sql_font: f.sqlFont.trim(),
    default_sql_font_size: String(n("sqlFontSize")),
    default_result_font_size: String(n("tableFontSize")),
    default_sql_query: f.defaultQuery.trim(),
    default_limit: String(n("defaultLimit")),
    warn_on_exit_with_transforms: f.warnTransforms,
    keep_running_in_tray: f.keepInTray,
    copy_tuple_sample_size: n("copySample"),
    query_memory_limit_mb: n("memoryLimit"),
    enable_crash_reporting: f.crashReporting,
    bug_report_email: f.bugEmail.trim(),
    enable_large_dataset_warning: f.warningEnabled,
    warning_criteria: f.criteria,
    detect_column_types: f.detectTypes,
    date_order: f.dateOrder,
    warning_threshold_rows: n("rowsThreshold"),
    warning_threshold_cells: n("cellsThreshold"),
    warning_threshold_filesize_mb: n("sizeThreshold"),
    show_query_panel: f.showQueryPanel,
    query_panel_collapsed: f.queryCollapsed,
    show_steps_panel: f.showSteps,
    show_ai_panel: f.showAi,
    remember_ai_chats: f.rememberAiChats,
  };
}

function SettingsDialog({ ui, close, a }: { ui: Ui; close: () => void; a: Callbacks }) {
  const t = ui.t;
  const initialLanguage = ui.settings.current_language || ui.boot.language;
  const initialTheme = ui.settings.current_theme || ui.boot.theme.name;
  const [tab, setTab] = useState<Tab>("general");
  const [form, setForm] = useState<SettingsForm>(() => settingsToForm(ui.settings, initialLanguage, initialTheme));
  const [themes, setThemes] = useState<ThemeDetails[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** Saved AI conversations (null until counted). */
  const [savedChats, setSavedChats] = useState<number | null>(null);

  const reloadThemes = async () => {
    try {
      setThemes(await api.listThemes());
    } catch (e) {
      setError(errorMessage(e));
    }
  };
  useEffect(() => {
    reloadThemes();
    api.aiSavedChats().then(setSavedChats, () => setSavedChats(null));
  }, []);

  const deleteSavedChats = async (question: string): Promise<void> => {
    if (!(await ui.confirm(t("settings.ai_chats.delete_title"), question))) return;
    try {
      const n = await api.aiDeleteSavedChats();
      setSavedChats(0);
      ui.status(t("settings.ai_chats.deleted", { count: n }), 4000);
    } catch (e) {
      await ui.alert(t("settings.ai_chats.delete_title"), errorMessage(e), "error");
    }
  };

  const set = <K extends keyof SettingsForm>(k: K, v: SettingsForm[K]) => setForm((f) => ({ ...f, [k]: v }));
  const invalid = invalidNumericFields(form);
  const canSave = invalid.length === 0 && !busy;

  const save = async () => {
    if (busy) return;
    if (invalid.length) return;
    setBusy(true);
    try {
      let saved = await api.updateSettings(formToValues(form));
      if (form.language !== initialLanguage) {
        const r = await api.setLanguage(form.language);
        a.onLanguage(r.language, r.translations);
        saved = { ...saved, current_language: r.language };
      }
      if (form.theme !== initialTheme) {
        const info = await api.setTheme(form.theme);
        a.onTheme(info);
        saved = { ...saved, current_theme: info.name };
      }
      a.onSettings(saved);
      close();
      // Turned off: offer to delete what was saved so far.
      if (ui.settings.remember_ai_chats !== false && !form.rememberAiChats) {
        const count = await api.aiSavedChats().catch(() => 0);
        if (count > 0) await deleteSavedChats(t("settings.ai_chats.off_confirm", { count }));
      }
    } catch (e) {
      setBusy(false);
      await ui.alert(t("settings.error.save_failed"), t("settings.error.save_failed_msg", { error: errorMessage(e) }), "error");
    }
  };

  const reset = async () => {
    if (!(await ui.confirm(t("settings.reset.title"), t("settings.reset.message")))) return;
    setBusy(true);
    try {
      let s = await api.resetSettings();
      if (s.current_language && s.current_language !== initialLanguage) {
        const r = await api.setLanguage(s.current_language);
        a.onLanguage(r.language, r.translations);
        s = { ...s, current_language: r.language };
      }
      if (s.current_theme && s.current_theme !== initialTheme) {
        const info = await api.setTheme(s.current_theme);
        a.onTheme(info);
        s = { ...s, current_theme: info.name };
      }
      a.onSettings(s);
      ui.status(t("settings.reset.done"), 4000);
      close();
    } catch (e) {
      setBusy(false);
      await ui.alert(t("settings.error.save_failed"), t("settings.error.save_failed_msg", { error: errorMessage(e) }), "error");
    }
  };

  const num = (k: keyof SettingsForm, suffix?: string, tooltip?: string, disabled?: boolean) => {
    const range = NUMERIC_RANGES.find(([x]) => x === k)!;
    return (
      <span className="row">
        <input
          type="number"
          min={range[1]}
          max={range[2]}
          value={form[k] as string}
          disabled={disabled}
          title={tooltip}
          aria-invalid={invalid.includes(k)}
          onChange={(e) => set(k, e.target.value as never)}
        />
        {suffix && <span className="muted">{suffix.trim()}</span>}
        {invalid.includes(k) && !disabled && (
          <span className="error-text small">{t("dialog.common.range", { min: range[1], max: range[2] })}</span>
        )}
      </span>
    );
  };
  const check = (k: keyof SettingsForm, label: string, tooltip?: string) => (
    <label className="row" title={tooltip}>
      <input type="checkbox" checked={form[k] as boolean} onChange={(e) => set(k, e.target.checked as never)} />
      {label}
    </label>
  );
  const field = (label: string, control: ReactNode) => (
    <>
      <label>{label}</label>
      {control}
    </>
  );

  const tabs: [Tab, string][] = [
    ["general", t("settings.tab.general")],
    ["theme", t("settings.tab.theme")],
    ["advanced", t("settings.tab.advanced")],
    ["warnings", t("settings.tab.warnings")],
  ];

  const general = (
    <>
      <fieldset>
        <legend>{t("settings.group.data")}</legend>
        <div className="form-grid">
          {field(t("settings.label.rows_per_page"), num("rowsPerPage", undefined, t("settings.tooltip.rows_per_page")))}
          {field(t("settings.label.max_rows"), num("maxRows", undefined, t("settings.tooltip.max_rows")))}
        </div>
      </fieldset>
      <fieldset>
        <legend>{t("settings.group.file_history")}</legend>
        {check("saveHistory", t("settings.label.save_history"), t("settings.tooltip.save_history"))}
      </fieldset>
      <fieldset>
        <legend>{t("settings.group.view")}</legend>
        {check("showQueryPanel", t("settings.label.show_query_panel"))}
        {check("queryCollapsed", t("settings.label.query_collapsed"))}
        {check("showSteps", t("settings.label.show_steps"))}
        {check("showAi", t("settings.label.show_ai"))}
        <div className="pv-hint">{t("settings.hint.view")}</div>
      </fieldset>
      <fieldset>
        <legend>{t("settings.group.ai")}</legend>
        {check("rememberAiChats", t("settings.label.remember_ai_chats"), t("settings.tooltip.remember_ai_chats"))}
        <div className="row" style={{ marginTop: 6 }}>
          <span className="muted small grow">
            {savedChats === null ? "" : t("settings.ai_chats.count", { count: savedChats })}
          </span>
          <button
            disabled={!savedChats}
            onClick={() =>
              savedChats && void deleteSavedChats(t("settings.ai_chats.delete_confirm", { count: savedChats }))
            }
          >
            {t("settings.ai_chats.delete_all")}
          </button>
        </div>
      </fieldset>
      <fieldset>
        <legend>{t("settings.group.exit")}</legend>
        {check("warnTransforms", t("settings.label.warn_transforms"))}
        {check("keepInTray", t("settings.label.keep_in_tray"), t("settings.tooltip.keep_in_tray"))}
      </fieldset>
      <fieldset>
        <legend>{t("settings.group.copy")}</legend>
        <div className="form-grid">
          {field(t("settings.label.copy_sample"), num("copySample", t("settings.suffix.rows")))}
        </div>
      </fieldset>
      <fieldset>
        <legend>{t("settings.group.column_types")}</legend>
        {check("detectTypes", t("settings.label.detect_types"), t("settings.tooltip.detect_types"))}
        <div className="form-grid" style={{ marginTop: 6 }}>
          {field(
            t("settings.label.date_order"),
            <select value={form.dateOrder} onChange={(e) => set("dateOrder", e.target.value as Settings["date_order"])}>
              <option value="auto">{t("settings.date_order.auto")}</option>
              <option value="day_first">{t("settings.date_order.day_first")}</option>
              <option value="month_first">{t("settings.date_order.month_first")}</option>
            </select>,
          )}
        </div>
        <div className="muted small">{t("settings.hint.column_types")}</div>
      </fieldset>
      <fieldset>
        <legend>{t("settings.group.performance")}</legend>
        <div className="form-grid">
          {field(t("settings.label.memory_limit"), num("memoryLimit", t("settings.suffix.mb")))}
        </div>
        <div className="muted small">{t("settings.hint.memory_limit")}</div>
      </fieldset>
      <fieldset>
        <legend>{t("settings.group.language")}</legend>
        <div className="form-grid">
          {field(
            t("language.label"),
            <LanguagePicker languages={ui.boot.languages} value={form.language} onChange={(v) => set("language", v)} />,
          )}
        </div>
      </fieldset>
    </>
  );

  const themeTab = (
    <fieldset className="col">
      <legend>{t("settings.group.theme_selection")}</legend>
      <div className="form-grid">
        {field(
          t("settings.label.active_theme"),
          <select value={form.theme} onChange={(e) => set("theme", e.target.value)}>
            {themes.length === 0 && <option value={form.theme}>{form.theme}</option>}
            {themes.map((th) => (
              <option key={th.name} value={th.name}>
                {th.name}
              </option>
            ))}
          </select>,
        )}
      </div>
      <div className="pv-section-title">{t("settings.group.theme_preview")}</div>
      <ThemePreview t={t} name={form.theme} details={themes.find((x) => x.name === form.theme)} />
      <div className="row">
        <button
          onClick={async () => {
            const name = await importThemeFlow(ui);
            if (name) {
              await reloadThemes();
              set("theme", name);
            }
          }}
        >
          {t("btn.import")}
        </button>
        <button onClick={() => exportThemeFlow(ui, form.theme)}>{t("btn.export")}</button>
      </div>
    </fieldset>
  );

  const advanced = (
    <>
      <fieldset>
        <legend>{t("settings.group.fonts")}</legend>
        <div className="form-grid">
          {field(
            t("settings.label.sql_font"),
            <input
              type="text"
              value={form.sqlFont}
              title={t("settings.tooltip.sql_font")}
              onChange={(e) => set("sqlFont", e.target.value)}
            />,
          )}
          {field(t("settings.label.sql_font_size"), num("sqlFontSize", undefined, t("settings.tooltip.sql_font_size")))}
          {field(t("settings.label.table_font_size"), num("tableFontSize", undefined, t("settings.tooltip.table_font_size")))}
        </div>
        <div className="pv-hint">{t("settings.note.fonts")}</div>
      </fieldset>
      <fieldset>
        <legend>{t("settings.group.sql")}</legend>
        <div className="form-grid">
          {field(
            t("settings.label.default_query"),
            <textarea
              className="pv-mono"
              rows={3}
              value={form.defaultQuery}
              title={t("settings.tooltip.default_query")}
              onChange={(e) => set("defaultQuery", e.target.value)}
            />,
          )}
          {field(t("settings.label.default_limit"), num("defaultLimit", undefined, t("settings.tooltip.default_limit")))}
        </div>
      </fieldset>
      <fieldset>
        <legend>{t("settings.group.crash")}</legend>
        {check("crashReporting", t("settings.label.crash_reporting"))}
        <div className="form-grid" style={{ marginTop: 6 }}>
          {field(
            t("settings.label.bug_email"),
            <input type="text" value={form.bugEmail} onChange={(e) => set("bugEmail", e.target.value)} />,
          )}
        </div>
      </fieldset>
    </>
  );

  const off = !form.warningEnabled;
  const criterion = (c: WarningCriteria, label: string, k: keyof SettingsForm, suffix: string, tooltip: string) => (
    <>
      <label className="row">
        <input type="radio" name="pv-criteria" disabled={off} checked={form.criteria === c} onChange={() => set("criteria", c)} />
        {label}
      </label>
      <div className="row pv-indent">
        <span>{t("settings.warning.threshold")}</span>
        {num(k, suffix, tooltip, off)}
      </div>
    </>
  );
  const warnings = (
    <>
      <fieldset>
        <legend>{t("settings.group.large_dataset")}</legend>
        {check("warningEnabled", t("settings.warning.enable"), t("settings.tooltip.enable_warning"))}
      </fieldset>
      <fieldset className="col">
        <legend>{t("settings.group.warning_criteria")}</legend>
        <span className={off ? "muted" : ""}>{t("settings.warning.trigger")}</span>
        {criterion("rows", t("settings.warning.row_count"), "rowsThreshold", t("settings.suffix.rows"), t("settings.tooltip.row_threshold"))}
        {criterion("cells", t("settings.warning.cell_count"), "cellsThreshold", t("settings.suffix.cells"), t("settings.tooltip.cell_threshold"))}
        {criterion("filesize", t("settings.warning.file_size"), "sizeThreshold", t("settings.suffix.mb"), t("settings.tooltip.size_threshold"))}
      </fieldset>
      <div className="pv-hint">{t("settings.note.warnings")}</div>
    </>
  );

  return (
    <Dialog
      title={t("dialog.settings")}
      onClose={close}
      onSubmit={() => canSave && save()}
      className="pv-dialog-wide"
      footer={
        <>
          <div className="left">
            <button disabled={busy} onClick={reset}>
              {t("settings.reset.button")}
            </button>
          </div>
          <button onClick={close}>{t("btn.cancel")}</button>
          <button className="primary" disabled={!canSave} onClick={save}>
            {t("btn.save")}
          </button>
        </>
      }
    >
      <div className="tabs-strip" role="tablist">
        {tabs.map(([id, label]) => (
          <button
            key={id}
            role="tab"
            aria-selected={tab === id}
            className={tab === id ? "active" : ""}
            onClick={() => setTab(id)}
          >
            {label}
          </button>
        ))}
      </div>
      {error && <div className="error-text">{error}</div>}
      <div className="pv-tab-pane" role="tabpanel">
        {tab === "general" && general}
        {tab === "theme" && themeTab}
        {tab === "advanced" && advanced}
        {tab === "warnings" && warnings}
      </div>
    </Dialog>
  );
}

export function openSettingsDialog(ui: Ui, a: Callbacks): Promise<void> {
  return ui.showDialog<void>((close) => <SettingsDialog ui={ui} close={() => close()} a={a} />, undefined);
}
