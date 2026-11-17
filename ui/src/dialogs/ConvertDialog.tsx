import { useEffect, useRef, useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import { api, errorMessage } from "../lib/api";
import type { Cell, Operation, Reading } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { RichText, useDebounced } from "./common";
import { convertChoices, sameReading } from "../lib/types";
import { buildConvertOp, conversionSummary } from "./logic";
import "./dialogs.css";

interface Args {
  tabId: number;
  table: string;
  column: string;
}

interface Count {
  errors: number;
  checked: number;
  complete: boolean;
}

const PREVIEW_ROWS = 50;

/**
 * Convert a column to another type, reading it with options (number and
 * date formats, yes/no words, Unix time), with a live preview and a count of
 * the values that don't convert. Power Query: Change Type, Using Locale…,
 * Keep Errors.
 */
function ConvertDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const choices = convertChoices();
  const [index, setIndex] = useState(0);
  const [suggested, setSuggested] = useState<number | null>(null);
  const [onError, setOnError] = useState<"empty" | "value">("empty");
  const [value, setValue] = useState("");
  const [keep, setKeep] = useState(false);
  const [preview, setPreview] = useState<{ before: Cell; after: Cell; failed: boolean }[]>([]);
  const [previewError, setPreviewError] = useState("");
  const [count, setCount] = useState<Count | null>(null);
  const [counting, setCounting] = useState(false);
  const reading: Reading = choices[index].reading;

  // Start from how ParVu would read the column.
  useEffect(() => {
    let live = true;
    api
      .suggestReading(a.tabId, a.column)
      .then((r) => {
        const i = choices.findIndex((c) => sameReading(c.reading, r));
        if (live && i >= 0) {
          setIndex(i);
          setSuggested(i);
        }
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
    // Once per dialog.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Preview and a sampled error count for the chosen reading.
  useDebounced(JSON.stringify(reading), 200, (key) => {
    const r = JSON.parse(key) as Reading;
    let live = true;
    setCount(null);
    api
      .previewConversion(a.tabId, a.column, r, PREVIEW_ROWS)
      .then((rows) => live && (setPreview(rows), setPreviewError("")))
      .catch((e) => live && (setPreview([]), setPreviewError(errorMessage(e))));
    const sample = api.conversionErrorCount(a.tabId, a.column, r, false);
    sample.result.then((c) => live && setCount(c)).catch(() => undefined);
    return () => {
      live = false;
      sample.cancel();
    };
  });

  const fullCount = useRef<(() => void) | null>(null);
  const countAll = async () => {
    setCounting(true);
    const job = api.conversionErrorCount(a.tabId, a.column, reading, true);
    fullCount.current = job.cancel;
    try {
      setCount(await job.result);
    } catch {
      // Stopped, or failed: keep the sampled count.
    } finally {
      fullCount.current = null;
      setCounting(false);
    }
  };
  const stop = () => fullCount.current?.();
  const finish = (op: Operation | null) => {
    if (counting) stop();
    close(op);
  };

  const op = buildConvertOp(a.column, reading, onError === "value" ? value : null, keep);
  const failedShown = preview.filter((r) => r.failed).length;
  const summary = conversionSummary(count);

  return (
    <Dialog
      title={t("dialog.convert.title")}
      onClose={() => finish(null)}
      onSubmit={() => op && finish(op)}
      className="pv-dialog-convert"
      footer={
        <>
          <div className="left">
            <button
              disabled={!count || count.errors === 0}
              title={t("dialog.convert.show_errors_hint")}
              onClick={() => finish({ op: "conversion_errors", column: a.column, to: reading, keep: true })}
            >
              {t("dialog.convert.show_errors")}
            </button>
          </div>
          <button onClick={() => finish(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={!op} onClick={() => op && finish(op)}>
            {t("dialog.convert.apply")}
          </button>
        </>
      }
    >
      <div>
        <RichText text={t("dialog.convert.column", { column: a.column, table: a.table })} />
      </div>
      <label className="row">
        {t("dialog.convert.read_as")}
        <select className="grow" value={index} onChange={(e) => setIndex(Number(e.target.value))}>
          {choices.map((c, i) => (
            <option key={i} value={i}>
              {t(c.key, c.args)}
              {i === suggested ? ` — ${t("dialog.convert.suggested")}` : ""}
            </option>
          ))}
        </select>
      </label>
      <div className="row">
        <span>{t("dialog.convert.on_error")}</span>
        <label className="row">
          <input type="radio" checked={onError === "empty"} onChange={() => setOnError("empty")} />
          {t("dialog.convert.on_error.empty")}
        </label>
        <label className="row">
          <input type="radio" checked={onError === "value"} onChange={() => setOnError("value")} />
          {t("dialog.convert.on_error.value")}
        </label>
        {onError === "value" && (
          <input type="text" className="grow" value={value} autoFocus onChange={(e) => setValue(e.target.value)} />
        )}
      </div>
      <label className="row">
        <input type="checkbox" checked={keep} onChange={(e) => setKeep(e.target.checked)} />
        {t("dialog.convert.keep_original", { column: `${a.column}_raw` })}
      </label>

      <div className="pv-table-scroll pv-convert-preview">
        <table className="simple">
          <thead>
            <tr>
              <th>{t("dialog.convert.before")}</th>
              <th>{t("dialog.convert.after")}</th>
            </tr>
          </thead>
          <tbody>
            {preview.map((r, i) => (
              <tr key={i} className={r.failed ? "pv-convert-failed" : undefined}>
                <td>{r.before ?? <span className="muted">{t("grid.null")}</span>}</td>
                <td>{r.failed ? t("dialog.convert.failed") : (r.after ?? <span className="muted">{t("grid.null")}</span>)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {previewError && <div className="error-text small">{previewError}</div>}
      <div className="row pv-convert-count">
        <span className={count && count.errors > 0 ? "pv-convert-warn" : "muted"}>
          {summary ? t(summary.key, summary.args) : t("dialog.convert.counting")}
          {failedShown > 0 && ` ${t("dialog.convert.failed_shown", { n: String(failedShown) })}`}
        </span>
        {count && !count.complete && !counting && (
          <button className="text" onClick={() => void countAll()}>
            {t("dialog.convert.count_all")}
          </button>
        )}
        {counting && (
          <button className="text" onClick={stop}>
            {t("dialog.convert.stop")}
          </button>
        )}
      </div>
    </Dialog>
  );
}

export function openConvert(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <ConvertDialog ui={ui} close={close} a={a} />, null);
}
