// Small building blocks shared by the dialogs.
import { Fragment, useEffect, useMemo, useRef } from "react";
import type { ReactNode } from "react";
// @ts-ignore -- marked 4 ships no type declarations
import { marked } from "marked";
import { openUrl } from "@tauri-apps/plugin-opener";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import type { Ui } from "../lib/ui";
import { errorMessage } from "../lib/api";

/**
 * Render a translated string that may hold the tiny HTML subset the locale
 * files use (`<b>`, `<br>`, `&nbsp;`, `<span ...>`). Everything else is text.
 */
export function RichText({ text }: { text: string }) {
  const parts: ReactNode[] = [];
  const re = /<\s*(\/?)\s*(b|br|span)\b[^>]*>|&nbsp;|&amp;|&lt;|&gt;/gi;
  const entities: Record<string, string> = { "&nbsp;": "\u00a0", "&amp;": "&", "&lt;": "<", "&gt;": ">" };
  let buf = "";
  let last = 0;
  let bold = false;
  const flush = () => {
    if (buf) parts.push(bold ? <b key={parts.length}>{buf}</b> : <Fragment key={parts.length}>{buf}</Fragment>);
    buf = "";
  };
  let m: RegExpExecArray | null;
  while ((m = re.exec(text))) {
    buf += text.slice(last, m.index);
    last = m.index + m[0].length;
    const entity = entities[m[0].toLowerCase()];
    if (entity !== undefined) {
      buf += entity;
      continue;
    }
    flush();
    if (m[2].toLowerCase() === "br") parts.push(<br key={parts.length} />);
    else if (m[2].toLowerCase() === "b") bold = m[1] !== "/";
    // <span> is dropped, its text kept
  }
  buf += text.slice(last);
  flush();
  return <>{parts}</>;
}

/** Qt's "&&" escape for a literal ampersand in button labels. */
export function qtLabel(s: string): string {
  return s.replace(/&&/g, "&");
}

const md = marked as { parse(src: string, opts?: Record<string, unknown>): string };

/**
 * Markdown from trusted sources (bundled locale files and generated text).
 * Links open in the system browser instead of navigating the webview.
 */
export function Markdown({ source, className }: { source: string; className?: string }) {
  const html = useMemo(() => md.parse(source, { mangle: false, headerIds: false }), [source]);
  return (
    <div
      className={`doc ${className ?? ""}`}
      onClick={(e) => {
        const a = (e.target as HTMLElement).closest("a");
        if (!a) return;
        e.preventDefault();
        const href = a.getAttribute("href");
        if (href && /^(https?|mailto):/i.test(href)) openUrl(href).catch(() => undefined);
      }}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

/**
 * Ref that focuses (and selects) its element once mounted. Runs after the
 * Dialog frame's own initial focus, so it wins.
 */
export function useAutoFocus<T extends HTMLElement>(select = false) {
  const ref = useRef<T>(null);
  useEffect(() => {
    const id = window.setTimeout(() => {
      const el = ref.current;
      if (!el) return;
      el.focus();
      if (select && (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) el.select();
    }, 0);
    return () => window.clearTimeout(id);
  }, [select]);
  return ref;
}

/**
 * Wraps a dialog body so Enter on a checkbox, radio or select also submits
 * (the Dialog frame only handles text inputs).
 */
export function SubmitOnEnter({ onSubmit, children }: { onSubmit: () => void; children: ReactNode }) {
  return (
    <div
      className="col grow pv-submit-scope"
      onKeyDown={(e) => {
        if (e.key !== "Enter" || e.shiftKey || e.ctrlKey || e.altKey) return;
        const el = e.target as HTMLElement;
        const isToggle = el instanceof HTMLInputElement && (el.type === "checkbox" || el.type === "radio");
        if (isToggle || el.tagName === "SELECT") {
          e.preventDefault();
          e.stopPropagation();
          onSubmit();
        }
      }}
    >
      {children}
    </div>
  );
}

export async function copyText(ui: Ui, text: string, okMessage?: string): Promise<boolean> {
  try {
    await writeText(text);
    if (okMessage) ui.status(okMessage, 3000);
    return true;
  } catch (e) {
    await ui.alert(ui.t("dialog.common.copy_failed"), errorMessage(e), "error");
    return false;
  }
}

/** Debounced copy of `value`. */
export function useDebounced<T>(value: T, ms: number, effect: (v: T) => void | (() => void)): void {
  const cb = useRef(effect);
  cb.current = effect;
  useEffect(() => {
    let cleanup: void | (() => void);
    const id = window.setTimeout(() => {
      cleanup = cb.current(value);
    }, ms);
    return () => {
      window.clearTimeout(id);
      if (typeof cleanup === "function") cleanup();
    };
  }, [value, ms]);
}

/** Checklist of columns with "select all" / "none" buttons. */
export function ColumnChecklist({
  ui,
  columns,
  checked,
  onChange,
  disabled,
}: {
  ui: Ui;
  columns: string[];
  checked: Set<string>;
  onChange: (next: Set<string>) => void;
  /** Columns that cannot be ticked (used elsewhere in the dialog). */
  disabled?: Set<string>;
}) {
  const toggle = (c: string) => {
    const n = new Set(checked);
    if (n.has(c)) n.delete(c);
    else n.add(c);
    onChange(n);
  };
  return (
    <div className="col">
      <div className="row">
        <button onClick={() => onChange(new Set(columns.filter((c) => !disabled?.has(c))))}>{ui.t("btn.select_all")}</button>
        <button onClick={() => onChange(new Set())}>{ui.t("btn.deselect_all")}</button>
      </div>
      <div className="list-box pv-column-checklist">
        {columns.map((c) => (
          <div key={c} className={`item ${checked.has(c) ? "selected" : ""}`}>
            <label className="row grow">
              <input type="checkbox" checked={checked.has(c)} disabled={disabled?.has(c)} onChange={() => toggle(c)} />
              {c}
            </label>
          </div>
        ))}
      </div>
    </div>
  );
}

/** Drop-down of columns; `emptyLabel` adds a "none" choice with value "". */
export function ColumnSelect({
  columns,
  value,
  onChange,
  emptyLabel,
  className,
}: {
  columns: string[];
  value: string;
  onChange: (v: string) => void;
  emptyLabel?: string;
  className?: string;
}) {
  return (
    <select className={className ?? "grow"} value={value} onChange={(e) => onChange(e.target.value)}>
      {emptyLabel !== undefined && <option value="">{emptyLabel}</option>}
      {columns.map((c) => (
        <option key={c} value={c}>
          {c}
        </option>
      ))}
    </select>
  );
}
