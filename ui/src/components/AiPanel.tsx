// The built-in AI assistant panel (right side of the window). The chat state
// lives in App (one per tab); this renders it and sends questions.
import { useEffect, useMemo, useRef, useState } from "react";
// @ts-ignore -- marked 4 ships no type declarations
import { marked } from "marked";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Ui } from "../lib/ui";
import { api, errorMessage } from "../lib/api";
import type { ProvidersInfo } from "../lib/api";
import { looksLikeKey, toolCallLabel, usageLabel, type AssistantPart, type ChatState } from "../lib/ai";
import { Icon } from "./Icon";

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

function Part({ ui, p }: { ui: Ui; p: AssistantPart }) {
  const t = ui.t;
  switch (p.type) {
    case "text":
      return <SafeMarkdown source={p.text} />;
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
      return (
        <details className={`ai-tool${p.isError ? " error" : ""}`}>
          <summary>
            <Icon name={p.result === undefined ? "run" : p.isError ? "close" : "check"} size={14} />
            <span className="mono ellipsis">{toolCallLabel(p.name, p.input)}</span>
          </summary>
          {p.result !== undefined && <pre className="pv-sql-preview">{p.result}</pre>}
        </details>
      );
  }
}

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
}) {
  const { ui, chat, providers } = props;
  const t = ui.t;
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const scroll = useRef<HTMLDivElement>(null);
  const running = chat.runId !== null;
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
        {chat.items.map((item, i) =>
          item.kind === "user" ? (
            <div key={i} className="ai-msg ai-user">
              {item.text}
            </div>
          ) : (
            <div key={i} className="ai-msg ai-assistant">
              {item.parts.map((p, j) => (
                <Part key={j} ui={ui} p={p} />
              ))}
              {!item.done && <div className="ai-typing" aria-label={t("ai.panel.working")} />}
              {item.stop?.type === "refusal" && <div className="warn-text">{t("ai.panel.refused")}</div>}
              {item.stop?.type === "max_tokens" && <div className="warn-text">{t("ai.panel.max_tokens")}</div>}
              {item.stop?.type === "other" && item.stop.reason === "tool_limit" && (
                <div className="warn-text">{t("ai.panel.tool_limit")}</div>
              )}
              {item.error && <div className="error-text">{item.error}</div>}
              {item.cancelled && <div className="muted small">{t("ai.panel.stopped")}</div>}
            </div>
          ),
        )}
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
