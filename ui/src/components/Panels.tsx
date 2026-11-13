// Smaller pieces of the main window: applied steps, pagination, tab bar,
// status bar and the welcome screen.
import { useEffect, useState } from "react";
import type { StepLabel, TabSummary } from "../lib/api";
import type { Translate } from "../lib/i18n";

/** Translate a step label; sort directions are translated too. */
export function stepText(t: Translate, label: StepLabel): string {
  const args: Record<string, string> = { ...label.args };
  if (label.key === "step.sort" && args.direction) args.direction = t(`step.direction.${args.direction}`);
  return t(label.key, args);
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
        <span className="chevron">{open ? "▾" : "▸"}</span>
        <span>
          {t("steps.title")} ({steps.length})
        </span>
        <span className="spacer" />
        <button
          className="small-btn"
          title={t("steps.undo_tooltip")}
          disabled={!steps.length || props.busy}
          onClick={(e) => {
            e.stopPropagation();
            props.onUndo();
          }}
        >
          ↩ {t("steps.undo")}
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
        ⏮
      </button>
      <button disabled={page <= 1 || props.busy} onClick={() => props.onPage(page - 1)}>
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
      <button disabled={page >= totalPages || props.busy} onClick={() => props.onPage(page + 1)}>
        {t("btn.next")}
      </button>
      <button
        className="icon"
        title={t("pagination.last")}
        disabled={page >= totalPages || props.busy}
        onClick={() => props.onPage(totalPages)}
      >
        ⏭
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
          {(tab.pending_edits > 0 || tab.steps.length > 0) && (
            <span className="dirty" title={t("tabs.unsaved")}>
              ●
            </span>
          )}
          <button
            className="tab-close"
            title={t("tabs.close")}
            onClick={(e) => {
              e.stopPropagation();
              props.onClose(tab.id);
            }}
          >
            ✕
          </button>
        </div>
      ))}
      <button className="tab-add" title={t("tabs.add")} onClick={props.onAdd}>
        ＋
      </button>
    </nav>
  );
}

export function Welcome(props: {
  t: Translate;
  recents: string[];
  extensions: string[];
  onOpen(path: string): void;
  onBrowse(): void;
}) {
  const { t } = props;
  const [path, setPath] = useState("");
  return (
    <div className="welcome">
      <div className="welcome-card col">
        <img src="/logo.png" alt="" className="welcome-logo" />
        <h2>{t("welcome.title")}</h2>
        <p className="muted">{t("welcome.hint")}</p>
        <div className="row">
          <input
            type="text"
            className="grow"
            placeholder={t("welcome.path_placeholder")}
            value={path}
            onChange={(e) => setPath(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && path.trim() && props.onOpen(path.trim())}
          />
          <button onClick={() => path.trim() && props.onOpen(path.trim())} disabled={!path.trim()}>
            {t("btn.reload")}
          </button>
          <button className="primary" onClick={props.onBrowse}>
            {t("welcome.open")}
          </button>
        </div>
        <div className="muted small">{props.extensions.join("  ")}</div>
        {props.recents.length > 0 && (
          <div className="col recents">
            <strong>{t("welcome.recent")}</strong>
            {props.recents.slice(0, 10).map((r) => (
              <button key={r} className="link ellipsis" title={r} onClick={() => props.onOpen(r)}>
                {r}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
