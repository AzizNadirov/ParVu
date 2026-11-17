// Smaller pieces of the main window: applied steps, pagination, tab bar,
// status bar and the welcome screen.
import { useEffect, useState } from "react";
import type { StepLabel, TabSummary } from "../lib/api";
import type { Translate } from "../lib/i18n";
import { Icon } from "./Icon";

/** Translate a step label; sort directions are translated too. */
export function stepText(t: Translate, label: StepLabel): string {
  const args: Record<string, string> = { ...label.args };
  if (label.key === "step.sort" && args.direction) args.direction = t(`step.direction.${args.direction}`);
  if (label.key === "step.slice" && args.mode) args.mode = t(`step.slice_mode.${args.mode}`);
  if (label.key === "step.window" && args.func) args.func = t(`window.${args.func}`);
  if (label.key === "step.fill_null" && args.method && !args.method.startsWith("'")) args.method = t(`fill.${args.method}`);
  const text = t(label.key, args);
  // Operations limited to some rows say which.
  return args.where ? `${text} ${t("step.where", { conditions: args.where })}` : text;
}

export function StepsPanel(props: {
  t: Translate;
  steps: StepLabel[];
  open: boolean;
  onToggle(): void;
  onUndo(): void;
  busy: boolean;
}) {
  const { t, steps, open } = props;
  return (
    <section className="steps-panel">
      <header className="panel-header" onClick={props.onToggle}>
        <Icon name="chevronDown" className={`chevron${open ? " open" : ""}`} />
        <span>{t("steps.title")}</span>
        <span className="count">{steps.length}</span>
        <span className="spacer" />
        <button
          className="small-btn text"
          title={t("steps.undo_tooltip")}
          disabled={!steps.length || props.busy}
          onClick={(e) => {
            e.stopPropagation();
            props.onUndo();
          }}
        >
          <Icon name="undo" />
          {t("steps.undo")}
        </button>
      </header>
      {open && (
        <ol className="steps-list">
          {steps.length === 0 && <li className="muted">{t("steps.empty")}</li>}
          {steps.map((s, i) => (
            <li key={i} className={i === steps.length - 1 ? "last" : ""} title={stepText(t, s)}>
              {stepText(t, s)}
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}

export function Pagination(props: {
  t: Translate;
  page: number;
  totalPages: number;
  totalRows: number;
  pageSize: number;
  onPage(p: number): void;
  busy: boolean;
}) {
  const { t, page, totalPages, totalRows, pageSize } = props;
  const [draft, setDraft] = useState(String(page));
  useEffect(() => setDraft(String(page)), [page]);
  const first = totalRows === 0 ? 0 : (page - 1) * pageSize + 1;
  const last = Math.min(totalRows, page * pageSize);
  const go = () => {
    const n = Math.round(Number(draft));
    if (Number.isFinite(n)) props.onPage(Math.min(Math.max(1, n), totalPages));
    else setDraft(String(page));
  };
  return (
    <div className="pagination">
      <button className="icon" title={t("pagination.first")} disabled={page <= 1 || props.busy} onClick={() => props.onPage(1)}>
        <Icon name="first" />
      </button>
      <button className="text" disabled={page <= 1 || props.busy} onClick={() => props.onPage(page - 1)}>
        {t("btn.previous")}
      </button>
      <span className="row">
        <input
          className="page-input"
          type="text"
          aria-label={t("pagination.go_to")}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && go()}
          onBlur={go}
        />
        <span className="nowrap">/ {totalPages.toLocaleString("en-US")}</span>
      </span>
      <button className="text" disabled={page >= totalPages || props.busy} onClick={() => props.onPage(page + 1)}>
        {t("btn.next")}
      </button>
      <button
        className="icon"
        title={t("pagination.last")}
        disabled={page >= totalPages || props.busy}
        onClick={() => props.onPage(totalPages)}
      >
        <Icon name="last" />
      </button>
      <span className="spacer" />
      <span className="muted nowrap">{t("pagination.range", { first, last, total_rows: totalRows })}</span>
    </div>
  );
}

export function TabBar(props: {
  t: Translate;
  tabs: TabSummary[];
  active: number | null;
  onSelect(id: number): void;
  onClose(id: number): void;
  onAdd(): void;
}) {
  const { t, tabs, active } = props;
  return (
    <nav className="tab-bar" role="tablist">
      {tabs.map((tab) => (
        <div
          key={tab.id}
          role="tab"
          aria-selected={tab.id === active}
          className={`tab${tab.id === active ? " active" : ""}`}
          title={tab.path}
          onMouseDown={(e) => {
            if (e.button === 1) {
              e.preventDefault();
              props.onClose(tab.id);
            }
          }}
          onClick={() => props.onSelect(tab.id)}
        >
          <span className="tab-name">{tab.name}</span>
          {(tab.pending_edits > 0 || tab.steps.length > 0 || tab.sheets_changed || tab.origin) && (
            <span className="dirty" title={t("tabs.unsaved")} />
          )}
          <button
            className="icon tab-close"
            title={t("tabs.close")}
            onClick={(e) => {
              e.stopPropagation();
              props.onClose(tab.id);
            }}
          >
            <Icon name="close" size={16} />
          </button>
        </div>
      ))}
      <button className="icon tab-add" title={t("tabs.add")} onClick={props.onAdd}>
        <Icon name="plus" />
      </button>
    </nav>
  );
}

/** How many recent files the start screen lists before "All N files". */
const HUB_RECENTS = 5;

/** A recent file as one line: format, name, then the folder (which is
 * what gets cut off). */
function RecentRow(props: { path: string; onOpen(path: string): void }) {
  const name = props.path.split(/[\\/]/).pop() ?? props.path;
  const dir = props.path.slice(0, Math.max(0, props.path.length - name.length - 1));
  const ext = (name.includes(".") ? name.split(".").pop() ?? "" : "").toUpperCase();
  return (
    <button className="hub-row" title={props.path} onClick={() => props.onOpen(props.path)}>
      <span className="hub-fmt">{ext || "—"}</span>
      <span className="hub-name ellipsis">{name}</span>
      <span className="hub-dir muted ellipsis">{dir}</span>
    </button>
  );
}

/** The start screen (no file open): open a file on the left, everything
 * else ParVu can do on the right. With no recent files, the left column
 * shows the three steps from a file to an answer. */
export function Welcome(props: {
  t: Translate;
  recents: string[];
  onOpen(path: string): void;
  onBrowse(): void;
  onBrowseWithOptions(): void;
  /** An AI model is set up (otherwise the assistant says "Add a model"). */
  aiReady: boolean;
  onAiAssistant(): void;
  onConnectAi(): void;
  onShortcuts(): void;
  onGuide(): void;
  onChanges(): void;
  themeName: string;
  languageName: string;
  onTheme(): void;
  onLanguage(): void;
}) {
  const { t } = props;
  const [path, setPath] = useState("");
  const [allRecents, setAllRecents] = useState(false);
  const recents = allRecents ? props.recents : props.recents.slice(0, HUB_RECENTS);
  const openPath = () => path.trim() && props.onOpen(path.trim());
  return (
    <div className="welcome hub">
      <div className="hub-grid">
        <div className="hub-main">
          <div className="hub-drop">
            <Icon name="open" size={40} />
            <div className="hub-big">{t("welcome.drop_title")}</div>
            <div className="muted">{t("welcome.drop_hint")}</div>
            <div className="hub-btns">
              <button className="primary" onClick={props.onBrowse}>
                <Icon name="open" />
                {t("welcome.open")}
                <kbd>Ctrl+O</kbd>
              </button>
              <button className="outlined" onClick={props.onBrowseWithOptions}>
                <Icon name="openOptions" />
                {t("menu.file.open_options")}
              </button>
            </div>
          </div>
          <label className="hub-path">
            <Icon name="paste" size={16} />
            <input
              type="text"
              className="grow"
              placeholder={t("welcome.path_placeholder")}
              aria-label={t("welcome.path_placeholder")}
              value={path}
              onChange={(e) => setPath(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && openPath()}
            />
          </label>
          {props.recents.length > 0 ? (
            <div className="hub-list">
              <div className="hub-sec">
                <strong>{t("welcome.recent")}</strong>
                {props.recents.length > HUB_RECENTS && (
                  <button className="text" onClick={() => setAllRecents((v) => !v)}>
                    {allRecents ? t("welcome.fewer_files") : t("welcome.all_files", { count: props.recents.length })}
                    <Icon name={allRecents ? "chevronUp" : "chevronDown"} size={16} />
                  </button>
                )}
              </div>
              {recents.map((r) => (
                <RecentRow key={r} path={r} onOpen={props.onOpen} />
              ))}
            </div>
          ) : (
            <div className="hub-list">
              <div className="hub-sec">
                <strong>{t("welcome.steps_title")}</strong>
              </div>
              <ol className="hub-steps">
                {[1, 2, 3].map((n) => (
                  <li key={n}>
                    <span className="hub-n">{n}</span>
                    <div>
                      <div className="hub-t">{t(`welcome.step${n}`)}</div>
                      <div className="muted small">{t(`welcome.step${n}_hint`)}</div>
                    </div>
                  </li>
                ))}
              </ol>
            </div>
          )}
        </div>
        <aside className="hub-side">
          <div className="hub-sec">
            <strong>{t("welcome.ai_title")}</strong>
          </div>
          <button className="hub-item" onClick={props.onAiAssistant}>
            <Icon name="ai" className="hub-lead" />
            <span className="hub-text">
              <span className="hub-t">{t("menu.ai.assistant")}</span>
              <span className="muted small">{t("welcome.ai_hint")}</span>
            </span>
            <span className="hub-state">{props.aiReady ? t("welcome.ai_ready") : t("welcome.ai_add_model")}</span>
          </button>
          <button className="hub-item" onClick={props.onConnectAi}>
            <Icon name="connect" className="hub-lead" />
            <span className="hub-text">
              <span className="hub-t">{t("menu.ai.connect")}</span>
              <span className="muted small">{t("welcome.connect_hint")}</span>
            </span>
            <Icon name="chevronRight" className="hub-tail" />
          </button>
          <div className="hub-sec">
            <strong>{t("welcome.learn_title")}</strong>
          </div>
          <button className="hub-item" onClick={props.onShortcuts}>
            <Icon name="keyboard" className="hub-lead" />
            <span className="hub-text">
              <span className="hub-t">{t("menu.help.shortcuts")}</span>
              <span className="muted small">{t("welcome.shortcuts_hint")}</span>
            </span>
            <kbd>Ctrl+/</kbd>
          </button>
          <button className="hub-item" onClick={props.onGuide}>
            <Icon name="book" className="hub-lead" />
            <span className="hub-text">
              <span className="hub-t">{t("menu.help.expression")}</span>
              <span className="muted small">{t("welcome.guide_hint")}</span>
            </span>
            <Icon name="chevronRight" className="hub-tail" />
          </button>
          <div className="hub-new">
            <div className="hub-sec">
              <strong>{t("welcome.new_title")}</strong>
              <button className="text" onClick={props.onChanges}>
                {t("welcome.all_changes")}
              </button>
            </div>
            <ul>
              <li>{t("welcome.new1")}</li>
              <li>{t("welcome.new2")}</li>
              <li>{t("welcome.new3")}</li>
            </ul>
          </div>
          <div className="hub-prefs">
            <button onClick={props.onTheme}>
              <Icon name="theme" />
              {props.themeName}
              <Icon name="chevronDown" size={16} />
            </button>
            <button onClick={props.onLanguage}>
              <Icon name="language" />
              {props.languageName}
              <Icon name="chevronDown" size={16} />
            </button>
          </div>
        </aside>
      </div>
    </div>
  );
}
