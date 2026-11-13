import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { ReplaceRequest } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { RichText, SubmitOnEnter, useAutoFocus } from "./common";
import { buildReplaceRequest, defaultReplaceOutput } from "./logic";
import "./dialogs.css";

interface Args {
  table: string;
  columns: string[];
  selected?: string;
}

function ReplaceDialog({ ui, close, a }: { ui: Ui; close: Closer<ReplaceRequest | null>; a: Args }) {
  const t = ui.t;
  const locked = !!a.selected && a.columns.includes(a.selected);
  const [column, setColumn] = useState(locked ? a.selected! : a.columns[0] ?? "");
  const [pattern, setPattern] = useState("");
  const [replacement, setReplacement] = useState("");
  const [output, setOutput] = useState(defaultReplaceOutput(column));
  const [caseSensitive, setCaseSensitive] = useState(true);
  const [regex, setRegex] = useState(false);
  const patternRef = useAutoFocus<HTMLInputElement>();

  const req = buildReplaceRequest({ column, pattern, replacement, output, caseSensitive, regex });
  const submit = () => req && close(req);
  const problem = !pattern ? t("dialog.replace.need_pattern") : !output.trim() ? t("dialog.replace.need_output") : null;

  return (
    <Dialog
      title={t("dialog.replace.title")}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-narrow"
      footer={
        <>
          <span className="left muted small">{problem}</span>
          <button onClick={() => close(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={!req} onClick={submit}>
            {t("btn.ok")}
          </button>
        </>
      }
    >
      <SubmitOnEnter onSubmit={submit}>
        <div>
          <RichText text={t("dialog.replace.table", { table: a.table })} />
        </div>
        <div className="form-grid">
          <label htmlFor="pv-replace-col">{t("dialog.replace.column")}</label>
          <select
            id="pv-replace-col"
            value={column}
            disabled={locked}
            onChange={(e) => {
              setColumn(e.target.value);
              setOutput(defaultReplaceOutput(e.target.value));
            }}
          >
            {a.columns.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>
          <label htmlFor="pv-replace-find">{t("dialog.replace.find")}</label>
          <input
            id="pv-replace-find"
            ref={patternRef}
            type="text"
            placeholder={t("dialog.replace.find_placeholder")}
            value={pattern}
            onChange={(e) => setPattern(e.target.value)}
          />
          <label htmlFor="pv-replace-with">{t("dialog.replace.replace_with")}</label>
          <input
            id="pv-replace-with"
            type="text"
            placeholder={t("dialog.replace.replace_placeholder")}
            value={replacement}
            onChange={(e) => setReplacement(e.target.value)}
          />
          <label htmlFor="pv-replace-out">{t("dialog.replace.output")}</label>
          <input id="pv-replace-out" type="text" value={output} onChange={(e) => setOutput(e.target.value)} />
        </div>
        <label className="row">
          <input type="checkbox" checked={caseSensitive} onChange={(e) => setCaseSensitive(e.target.checked)} />
          {t("dialog.replace.case_sensitive")}
        </label>
        <label className="row">
          <input type="checkbox" checked={regex} onChange={(e) => setRegex(e.target.checked)} />
          {t("dialog.replace.regex")}
        </label>
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openReplace(ui: Ui, a: Args): Promise<ReplaceRequest | null> {
  return ui.showDialog<ReplaceRequest | null>((close) => <ReplaceDialog ui={ui} close={close} a={a} />, null);
}
