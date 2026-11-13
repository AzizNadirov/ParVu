// Port of expression_dialog.py, used for "Math Operation": a new column
// computed from a DSL expression, with a live SQL preview.
import { useRef, useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { ApiError } from "../lib/api";
import { api, errorMessage } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { useAutoFocus, useDebounced } from "./common";
import { columnRef, defaultMathExpression, formatSignature, insertAt } from "./logic";
import "./dialogs.css";

interface Args {
  tabId: number;
  table: string;
  columns: string[];
}
type Result = { name: string; text: string } | null;

type Preview =
  | { state: "empty" }
  | { state: "pending" }
  | { state: "ok"; text: string; sql: string }
  | { state: "error"; text: string; message: string };

function describeError(e: unknown): string {
  const err = e as Partial<ApiError>;
  const msg = errorMessage(e);
  if (err && typeof err === "object" && err.line != null && err.column != null && !msg.includes(`${err.line}`)) {
    return `${msg} (${err.line}:${err.column})`;
  }
  return msg;
}

function MathDialog({ ui, close, a }: { ui: Ui; close: Closer<Result>; a: Args }) {
  const t = ui.t;
  const [name, setName] = useState("");
  const [text, setText] = useState(() => defaultMathExpression(a.table, a.columns));
  const [preview, setPreview] = useState<Preview>({ state: "empty" });
  const [submitting, setSubmitting] = useState(false);
  const nameRef = useAutoFocus<HTMLInputElement>();
  const exprRef = useRef<HTMLTextAreaElement>(null);
  const latest = useRef(0);

  const check = async (value: string): Promise<Preview> => {
    const trimmed = value.trim();
    if (!trimmed) return { state: "empty" };
    try {
      const p = await api.previewExpression(a.tabId, trimmed);
      return { state: "ok", text: trimmed, sql: p.sql };
    } catch (e) {
      return { state: "error", text: trimmed, message: describeError(e) };
    }
  };

  useDebounced(text, 250, (value) => {
    const id = ++latest.current;
    if (!value.trim()) {
      setPreview({ state: "empty" });
      return;
    }
    setPreview({ state: "pending" });
    check(value).then((p) => {
      if (id === latest.current) setPreview(p);
    });
  });

  const insert = (snippet: string, caretBack = 0) => {
    const el = exprRef.current;
    const start = el?.selectionStart ?? text.length;
    const end = el?.selectionEnd ?? text.length;
    const [next, caret] = insertAt(text, start, end, snippet, caretBack);
    setText(next);
    window.setTimeout(() => {
      el?.focus();
      el?.setSelectionRange(caret, caret);
    }, 0);
  };

  const nameOk = name.trim().length > 0;
  const textOk = text.trim().length > 0;
  const hasError = preview.state === "error" && preview.text === text.trim();
  const canSubmit = nameOk && textOk && !hasError && !submitting;

  const submit = async () => {
    if (!canSubmit) return;
    setSubmitting(true);
    const p = preview.state === "ok" && preview.text === text.trim() ? preview : await check(text);
    setSubmitting(false);
    if (p.state === "ok") close({ name: name.trim(), text: text.trim() });
    else setPreview(p);
  };

  const problem = !nameOk ? t("dialog.math.need_name") : !textOk ? t("dialog.math.need_expression") : null;

  return (
    <Dialog
      title={t("dialog.math.title")}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-medium"
      footer={
        <>
          <span className="left muted small">{problem}</span>
          <button onClick={() => close(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={!canSubmit} onClick={submit}>
            {t("btn.ok")}
          </button>
        </>
      }
    >
      <label className="row">
        {t("dialog.math.column_name")}
        <input
          ref={nameRef}
          type="text"
          className="grow"
          placeholder={t("dialog.math.column_placeholder")}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </label>
      <div className="row">
        <span className="grow">{t("dialog.math.expression")}</span>
        <select
          aria-label={t("dialog.math.insert_column")}
          value=""
          onChange={(e) => e.target.value && insert(columnRef(a.table, e.target.value))}
        >
          <option value="">{t("dialog.math.insert_column")}</option>
          {a.columns.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
        <select
          aria-label={t("dialog.math.insert_function")}
          value=""
          onChange={(e) => e.target.value && insert(`${e.target.value}()`, 1)}
        >
          <option value="">{t("dialog.math.insert_function")}</option>
          {ui.boot.functions.map((f) => (
            <option key={f.name} value={f.name} title={f.description}>
              {formatSignature(f)}
            </option>
          ))}
        </select>
      </div>
      <textarea
        ref={exprRef}
        className="pv-expr-input"
        spellCheck={false}
        value={text}
        placeholder={`${columnRef(a.table, "price")} * ${columnRef(a.table, "qty")}`}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            submit();
          }
        }}
      />
      <div className="pv-hint">{t("dialog.math.hint", { table: a.table })}</div>
      <div className="pv-section-title">{t("dialog.math.preview")}</div>
      <div className="pv-sql-preview" aria-live="polite">
        {preview.state === "ok"
          ? preview.sql
          : preview.state === "pending"
            ? t("dialog.math.compiling")
            : preview.state === "error"
              ? t("dialog.math.invalid")
              : t("dialog.math.enter_expression")}
      </div>
      {preview.state === "error" && <div className="error-text">{preview.message}</div>}
    </Dialog>
  );
}

export function openMathDialog(ui: Ui, a: Args): Promise<Result> {
  return ui.showDialog<Result>((close) => <MathDialog ui={ui} close={close} a={a} />, null);
}
