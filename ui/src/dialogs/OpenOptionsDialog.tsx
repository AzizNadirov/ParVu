import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import { api, errorMessage } from "../lib/api";
import type { CsvOptions, CsvPreview } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { useDebounced } from "./common";
import { csvOptionsFrom, delimiterLabel, type OpenForm } from "./logic";
import "./dialogs.css";

const PREVIEW_ROWS = 20;

/** Open a CSV file with options: delimiter, header row, rows to skip,
 * decimal separator, encoding; with a preview of the first rows. */
function OpenOptionsDialog({ ui, close, path }: { ui: Ui; close: Closer<CsvOptions | null>; path: string }) {
  const t = ui.t;
  const [form, setForm] = useState<OpenForm>({ delimiter: "auto", custom: "", header: "auto", skip: "", decimal: "auto", encoding: "auto" });
  const [preview, setPreview] = useState<CsvPreview | null>(null);
  const [error, setError] = useState("");
  const options = csvOptionsFrom(form);
  const set = <K extends keyof OpenForm>(k: K, v: OpenForm[K]) => setForm((f) => ({ ...f, [k]: v }));

  useDebounced(JSON.stringify(options), 250, (key) => {
    const o = JSON.parse(key) as CsvOptions | null;
    if (!o) return;
    let live = true;
    api
      .previewCsv(path, o)
      .then((p) => live && (setPreview(p), setError("")))
      .catch((e) => live && (setPreview(null), setError(errorMessage(e))));
    return () => {
      live = false;
    };
  });

  const d = preview?.detected;
  const submit = () => options && close(options);
  const name = path.split(/[\\/]/).pop() ?? path;

  return (
    <Dialog
      title={t("open_options.title")}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-convert"
      footer={
        <>
          <button onClick={() => close(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={!options || !!error} onClick={submit}>
            {t("open_options.open")}
          </button>
        </>
      }
    >
      <div className="muted small pv-mono">{name}</div>
      <div className="pv-open-grid">
        <label>{t("open_options.delimiter")}</label>
        <div className="row">
          <select value={form.delimiter} onChange={(e) => set("delimiter", e.target.value)}>
            <option value="auto">{t("open_options.auto")}</option>
            {[",", ";", "\t", "|"].map((c) => (
              <option key={c} value={c}>
                {delimiterLabel(c, t)}
              </option>
            ))}
            <option value="custom">{t("open_options.custom")}</option>
          </select>
          {form.delimiter === "custom" && (
            <input type="text" className="pv-mono" style={{ width: 70 }} value={form.custom} onChange={(e) => set("custom", e.target.value)} />
          )}
        </div>
        <label>{t("open_options.header")}</label>
        <select value={form.header} onChange={(e) => set("header", e.target.value as OpenForm["header"])}>
          <option value="auto">{t("open_options.auto")}</option>
          <option value="yes">{t("open_options.header_yes")}</option>
          <option value="no">{t("open_options.header_no")}</option>
        </select>
        <label>{t("open_options.skip")}</label>
        <input type="number" min={0} placeholder={t("open_options.auto")} value={form.skip} onChange={(e) => set("skip", e.target.value)} />
        <label>{t("open_options.decimal")}</label>
        <select value={form.decimal} onChange={(e) => set("decimal", e.target.value)}>
          <option value="auto">{t("open_options.decimal_point")}</option>
          <option value=",">{t("open_options.decimal_comma")}</option>
        </select>
        <label>{t("open_options.encoding")}</label>
        <select value={form.encoding} onChange={(e) => set("encoding", e.target.value)}>
          <option value="auto">UTF-8</option>
          <option value="utf-16">UTF-16</option>
          <option value="latin-1">Latin-1 (Windows-1252)</option>
        </select>
      </div>
      {d && (
        <div className="muted small">
          {t("open_options.detected", {
            delimiter: d.delimiter ? delimiterLabel(d.delimiter, t) : "—",
            header: t(d.header ? "open_options.header_yes" : "open_options.header_no"),
            skip: String(d.skip ?? 0),
          })}
        </div>
      )}
      {preview && (
        <div className="pv-table-scroll pv-convert-preview">
          <table className="simple">
            <thead>
              <tr>
                {preview.columns.map((c) => (
                  <th key={c.name} title={c.dtype}>
                    {c.name}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {preview.rows.slice(0, PREVIEW_ROWS).map((r, i) => (
                <tr key={i}>
                  {r.map((v, j) => (
                    <td key={j}>{v ?? <span className="muted">{t("grid.null")}</span>}</td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {error && <div className="error-text small">{error}</div>}
    </Dialog>
  );
}

export function openOpenOptions(ui: Ui, path: string): Promise<CsvOptions | null> {
  return ui.showDialog<CsvOptions | null>((close) => <OpenOptionsDialog ui={ui} close={close} path={path} />, null);
}
