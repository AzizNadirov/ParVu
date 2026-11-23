// The built-in AI assistant panel (right side of the window). The chat state
// lives in App (one per tab); this renders it and sends questions.
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
// @ts-ignore -- marked 4 ships no type declarations
import { marked } from "marked";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Ui } from "../lib/ui";
import { api, errorMessage } from "../lib/api";
import type { ProvidersInfo, TabSummary } from "../lib/api";
import {
  answerText,
  looksLikeKey,
  markdownSegments,
  restoredDivider,
  sqlEditBlocker,
  toolCallLabel,
  toolResultCopyText,
  toolResultSql,
  toolResultTable,
  usageLabel,
  type AssistantPart,
  type ChatState,
  type ResultTable,
} from "../lib/ai";
import { sqlSpans } from "../lib/sqlHighlight";
import { copyText } from "../dialogs/common";
import { Icon, type IconName } from "./Icon";

type MarkedApi = {
  parse(src: string, opts?: Record<string, unknown>): string;
  Renderer: new () => Record<string, unknown>;
};
const md = marked as unknown as MarkedApi;

const escapeHtml = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** Markdown from the model: raw HTML in it is shown as text, never run. */
const safeRenderer = (() => {
  const r = new md.Renderer();
  r.html = (html: string) => escapeHtml(html);
  return r;
})();

export function SafeMarkdown({ source }: { source: string }) {
  const html = useMemo(() => md.parse(source, { mangle: false, headerIds: false, renderer: safeRenderer }), [source]);
  return (
    <div
      className="doc ai-md"
      onClick={(e) => {
        const a = (e.target as HTMLElement).closest("a");
        if (!a) return;
        e.preventDefault();
        const href = a.getAttribute("href");
        if (href && /^https?:/i.test(href)) openUrl(href).catch(() => undefined);
      }}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

/** What the panel's SQL and result buttons do (wired up in App). */
export interface AiActions {
  /** Open tabs, to tell whether SQL can go into the SQL editor as it is. */
  tabs: TabSummary[];
  /** Run the SQL again (tables = the tabs' current results) into a new tab;
   * settles when the tab is open (or the run failed). `null`: no table open. */
  openSql: ((sql: string) => Promise<unknown> | void) | null;
  /** Put the SQL into the current tab's SQL editor without running it. */
  editSql: ((sql: string) => void) | null;
}

function ActionButton(props: { icon: IconName; label: string; title?: string; disabled?: boolean; onClick: () => void }) {
  const button = (
    <button className="text ai-action" disabled={props.disabled} title={props.title ?? props.label} onClick={props.onClick}>
      <Icon name={props.icon} size={14} />
      {props.label}
    </button>
  );
  // A disabled button gets no hover, so its reason goes on a wrapper.
  return props.disabled ? <span title={props.title ?? props.label}>{button}</span> : button;
}

/** "Open in new tab": the query runs again as a job, which can take a while
 * on a big table, so the button shows a spinner and "Opening…" until the
 * tab is open, and can't be clicked twice. */
function OpenTabButton({ t, sql, openSql }: { t: Ui["t"]; sql: string; openSql: AiActions["openSql"] }) {
  const [busy, setBusy] = useState(false);
  const mounted = useRef(true);
  useEffect(() => () => void (mounted.current = false), []);
  const open = async () => {
    if (!openSql || busy) return;
    setBusy(true);
    try {
      await openSql(sql);
    } finally {
      if (mounted.current) setBusy(false);
    }
  };
  return (
    <button
      className="text ai-action"
      disabled={!openSql || busy}
      aria-busy={busy}
      title={busy ? t("ai.action.opening_hint") : t("ai.action.open_tab_hint")}
      onClick={() => void open()}
    >
      {busy ? <span className="ai-spinner" aria-hidden="true" /> : <Icon name="newTable" size={14} />}
      {busy ? t("ai.action.opening") : t("ai.action.open_tab")}
    </button>
  );
}

/** SQL, read-only, highlighted like the SQL editor (spans of text, never HTML). */
export function SqlView({ sql }: { sql: string }) {
  const spans = useMemo(() => sqlSpans(sql), [sql]);
  return (
    <pre className="pv-sql-preview ai-sql">
      {spans.map((s, i) => (s.cls ? <span key={i} className={s.cls}>{s.text}</span> : s.text))}
    </pre>
  );
}

/** Copy, Open result in a new tab, Edit in SQL editor. */
function SqlActions({ ui, sql, actions, extra }: { ui: Ui; sql: string; actions: AiActions; extra?: ReactNode }) {
  const t = ui.t;
  const blocker = useMemo(() => sqlEditBlocker(sql, actions.tabs), [sql, actions.tabs]);
  return (
    <div className="ai-actions row">
      <ActionButton icon="copy" label={t("ai.action.copy_sql")} onClick={() => void copyText(ui, sql, t("dialog.common.copied"))} />
      <OpenTabButton t={t} sql={sql} openSql={actions.openSql} />
      <ActionButton
        icon="edit"
        label={t("ai.action.edit_sql")}
        title={blocker ? t("ai.action.edit_sql_blocked", { table: blocker }) : t("ai.action.edit_sql_hint")}
        disabled={!actions.editSql || blocker !== null}
        onClick={() => actions.editSql?.(sql)}
      />
      {extra}
    </div>
  );
}

function ResultView({ t, table }: { t: Ui["t"]; table: ResultTable }) {
  return (
    <div className="ai-result">
      <table className="simple">
        <thead>
          <tr>
            {table.columns.map((c, i) => (
              <th key={i}>{c}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {table.rows.map((r, i) => (
            <tr key={i}>
              {r.map((c, j) => (
                <td key={j} className={c === null ? "muted" : undefined}>
                  {c ?? "NULL"}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      <div className="muted small">
        {table.truncated ? t("ai.result.truncated", { rows: table.rows.length }) : t("ai.result.rows", { rows: table.rows.length })}
      </div>
    </div>
  );
}

function ToolPart({ ui, p, actions }: { ui: Ui; p: Extract<AssistantPart, { type: "tool" }>; actions: AiActions }) {
  const t = ui.t;
  const [open, setOpen] = useState(false);
  const input = (p.input && typeof p.input === "object" ? p.input : {}) as Record<string, unknown>;
  const sql = p.name === "query" && typeof input.sql === "string" ? input.sql : null;
  const table = useMemo(() => (open && !p.isError ? toolResultTable(p.name, p.result) : null), [open, p.isError, p.name, p.result]);
  const done = p.result !== undefined && !p.isError;
  const copyResult = done && p.result ? (
    <ActionButton
      icon="copy"
      label={t("ai.action.copy_result")}
      title={t("ai.action.copy_result_hint")}
      onClick={() => void copyText(ui, toolResultCopyText(p.name, p.result ?? ""), t("dialog.common.copied"))}
    />
  ) : null;
  const resultSql = done && p.name !== "query" ? toolResultSql(p.name, p.input) : null;
  const args = Object.entries(input).filter(([k]) => !(sql !== null && k === "sql"));
  return (
    <details className={`ai-tool${p.isError ? " error" : ""}`} onToggle={(e) => setOpen((e.currentTarget as HTMLDetailsElement).open)}>
      <summary>
        <Icon name={p.result === undefined ? "run" : p.isError ? "close" : "check"} size={14} />
        <span className="mono ellipsis">{toolCallLabel(p.name, p.input)}</span>
      </summary>
      {open && (
        <div className="ai-tool-body col">
          {sql !== null && <SqlView sql={sql} />}
          {args.length > 0 && (
            <pre className="pv-sql-preview ai-args">{args.map(([k, v]) => `${k}: ${typeof v === "string" ? v : JSON.stringify(v)}`).join("\n")}</pre>
          )}
          {sql !== null ? (
            <SqlActions ui={ui} sql={sql} actions={actions} extra={copyResult} />
          ) : (
            (copyResult || resultSql) && (
              <div className="ai-actions row">
                {resultSql && (
                  <OpenTabButton t={t} sql={resultSql} openSql={actions.openSql} />
                )}
                {copyResult}
              </div>
            )
          )}
          {p.result !== undefined && (table ? <ResultView t={t} table={table} /> : <pre className="pv-sql-preview">{p.result}</pre>)}
        </div>
      )}
    </details>
  );
}

/** A code block of an answer: SQL gets the SQL actions, anything else Copy. */
function CodeBlock({ ui, lang, text, sql, actions }: { ui: Ui; lang: string; text: string; sql: boolean; actions: AiActions }) {
  const t = ui.t;
  return (
    <div className="ai-code col">
      {sql ? <SqlView sql={text} /> : <pre className="pv-sql-preview" data-lang={lang || undefined}>{text}</pre>}
      {sql ? (
        <SqlActions ui={ui} sql={text} actions={actions} />
      ) : (
        <div className="ai-actions row">
          <ActionButton icon="copy" label={t("ai.action.copy")} onClick={() => void copyText(ui, text, t("dialog.common.copied"))} />
        </div>
      )}
    </div>
  );
}

/** An answer's Markdown, with its top-level code blocks as React blocks with buttons. */
function AnswerText({ ui, text, actions }: { ui: Ui; text: string; actions: AiActions }) {
  const segments = useMemo(() => markdownSegments(text), [text]);
  return (
    <>
      {segments.map((s, i) =>
        s.kind === "md" ? (
          <SafeMarkdown key={i} source={s.source} />
        ) : (
          <CodeBlock key={i} ui={ui} lang={s.lang} text={s.text} sql={s.sql} actions={actions} />
        ),
      )}
    </>
  );
}

function Part({ ui, p, actions }: { ui: Ui; p: AssistantPart; actions: AiActions }) {
  const t = ui.t;
  switch (p.type) {
    case "text":
      return <AnswerText ui={ui} text={p.text} actions={actions} />;
    case "thinking":
      return (
        <details className="ai-thinking">
          <summary>{t("ai.panel.thinking")}</summary>
          <div className="pv-hint">{p.text}</div>
        </details>
      );
    case "fallback":
      return <div className="pv-hint">{t("ai.panel.fallback", { from: p.from, to: p.to })}</div>;
    case "tool":
      return <ToolPart ui={ui} p={p} actions={actions} />;
  }
}

const NO_ACTIONS: AiActions = { tabs: [], openSql: null, editSql: null };

export function AiPanel(props: {
  ui: Ui;
  tabId: number | null;
  tabName: string | null;
  chat: ChatState;
  onStart: (question: string, runId: number) => void;
  onClear: () => void;
  providers: ProvidersInfo | null;
  providerId: string | null;
  onProvider: (id: string) => void;
  onOpenProviders: () => void;
  onConnectClient: () => void;
  onClose: () => void;
  /** The SQL and result buttons; without them, only Copy works. */
  actions?: AiActions;
}) {
  const { ui, chat, providers } = props;
  const actions = props.actions ?? NO_ACTIONS;
  const t = ui.t;
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const scroll = useRef<HTMLDivElement>(null);
  const running = chat.runId !== null;
  const divider = restoredDivider(chat);
  const noProviders = !!providers && providers.providers.length === 0;

  useEffect(() => {
    const el = scroll.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [chat]);

  const send = async () => {
    const q = draft.trim();
    if (!q || running || sending || !props.providerId) return;
    if (looksLikeKey(q) && !(await ui.confirm(t("ai.panel.key_warning_title"), t("ai.panel.key_warning")))) return;
    setSending(true);
    try {
      const runId = await api.aiAsk(props.providerId, props.tabId, q);
      props.onStart(q, runId);
      setDraft("");
    } catch (e) {
      await ui.alert(t("ai.panel.title"), errorMessage(e), "error");
    } finally {
      setSending(false);
    }
  };

  return (
    <aside className="ai-panel" aria-label={t("ai.panel.title")}>
      <header className="ai-panel-head row">
        <Icon name="ai" />
        <strong className="grow ellipsis">{t("ai.panel.title")}</strong>
        <button className="icon" title={t("ai.panel.clear")} disabled={running || !chat.items.length} onClick={props.onClear}>
          <Icon name="reset" size={18} />
        </button>
        <button className="icon" title={t("menu.ai.providers")} onClick={props.onOpenProviders}>
          <Icon name="settings" size={18} />
        </button>
        <button className="icon" title={t("btn.close")} onClick={props.onClose}>
          <Icon name="close" size={18} />
        </button>
      </header>
      {providers && providers.providers.length > 0 && (
        <div className="row ai-provider-row">
          <select
            className="grow"
            aria-label={t("ai.panel.provider")}
            value={props.providerId ?? ""}
            onChange={(e) => props.onProvider(e.target.value)}
            disabled={running}
          >
            {providers.providers.map((p) => (
              <option key={p.config.id} value={p.config.id}>
                {p.config.display_name} · {p.config.model}
              </option>
            ))}
          </select>
        </div>
      )}
      <div className="ai-messages" ref={scroll}>
        {noProviders && (
          <div className="ai-empty col">
            <strong>{t("ai.panel.no_provider_title")}</strong>
            <div className="pv-hint">{t("ai.panel.no_provider")}</div>
            <button className="primary" onClick={props.onOpenProviders}>
              {t("ai.panel.add_provider")}
            </button>
            <div className="pv-hint">{t("ai.panel.subscription_hint")}</div>
            <button className="text" onClick={props.onConnectClient}>
              {t("menu.ai.connect")}
            </button>
          </div>
        )}
        {!noProviders && chat.items.length === 0 && (
          <div className="ai-empty pv-hint">
            {props.tabName ? t("ai.panel.empty", { table: props.tabName }) : t("ai.panel.empty_no_table")}
          </div>
        )}
        {chat.items.map((item, i) => [
          divider && i === 0 && (
            <div key="earlier" className="ai-divider muted small" role="separator">
              {t("ai.panel.earlier", { date: new Date(divider.updatedMs).toLocaleString() })}
            </div>
          ),
          divider && i === divider.after && <hr key="earlier-end" className="ai-divider-end" />,
          item.kind === "user" ? (
            <div key={i} className="ai-msg ai-user">
              {item.text}
            </div>
          ) : (
            <div key={i} className="ai-msg ai-assistant">
              {item.parts.map((p, j) => (
                <Part key={j} ui={ui} p={p} actions={actions} />
              ))}
              {!item.done && <div className="ai-typing" aria-label={t("ai.panel.working")} />}
              {item.stop?.type === "refusal" && <div className="warn-text">{t("ai.panel.refused")}</div>}
              {item.stop?.type === "max_tokens" && <div className="warn-text">{t("ai.panel.max_tokens")}</div>}
              {item.stop?.type === "other" && item.stop.reason === "tool_limit" && (
                <div className="warn-text">{t("ai.panel.tool_limit")}</div>
              )}
              {item.error && <div className="error-text">{item.error}</div>}
              {item.cancelled && <div className="muted small">{t("ai.panel.stopped")}</div>}
              {item.done && answerText(item.parts) && (
                <div className="ai-answer-actions row">
                  <button
                    className="icon ai-copy-answer"
                    title={t("ai.action.copy_answer")}
                    aria-label={t("ai.action.copy_answer")}
                    onClick={() => void copyText(ui, answerText(item.parts), t("dialog.common.copied"))}
                  >
                    <Icon name="copy" size={16} />
                  </button>
                </div>
              )}
            </div>
          ),
        ])}
      </div>
      <footer className="ai-input col">
        <textarea
          rows={3}
          value={draft}
          disabled={noProviders}
          placeholder={t("ai.panel.placeholder")}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void send();
            }
          }}
        />
        <div className="row">
          <span className="muted small grow">{chat.items.length ? usageLabel(chat) : t("ai.panel.enter_hint")}</span>
          {running ? (
            <button onClick={() => chat.runId !== null && void api.aiCancel(chat.runId, props.tabId)}>{t("ai.panel.stop")}</button>
          ) : (
            <button className="primary" disabled={!draft.trim() || sending || !props.providerId} onClick={() => void send()}>
              {t("ai.panel.send")}
            </button>
          )}
        </div>
      </footer>
    </aside>
  );
}
