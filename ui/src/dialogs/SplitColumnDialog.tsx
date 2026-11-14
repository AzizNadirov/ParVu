import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { ColumnMeta, Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { ColumnSelect, RichText, SubmitOnEnter } from "./common";
import { buildSplitOp, isTextType } from "./logic";
import "./dialogs.css";

interface Args {
  table: string;
  columns: ColumnMeta[];
  selected?: string;
}

/** pandas str.split(expand=True); Calc's Text to Columns. */
function SplitColumnDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const names = a.columns.map((c) => c.name);
  const [column, setColumn] = useState(a.selected ?? a.columns.find((c) => isTextType(c.dtype))?.name ?? names[0] ?? "");
  const [separator, setSeparator] = useState(",");
  const [regex, setRegex] = useState(false);
  const [auto, setAuto] = useState(true);
  const [parts, setParts] = useState("2");
  const [keep, setKeep] = useState(false);
  const op = buildSplitOp(column, separator, regex, auto ? null : Number(parts), keep);
  const submit = () => op && close(op);

  return (
    <Dialog
      title={t("dialog.split.title")}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-narrow"
      footer={
        <>
          <button onClick={() => close(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={!op} onClick={submit}>
            {t("btn.apply")}
          </button>
        </>
      }
    >
      <SubmitOnEnter onSubmit={submit}>
        <div>
          <RichText text={t("dialog.common.table", { table: a.table })} />
        </div>
        <label className="row">
          {t("dialog.common.column")}
          <ColumnSelect columns={names} value={column} onChange={setColumn} />
        </label>
        <label className="row">
          {t("dialog.split.separator")}
          <input type="text" className="grow pv-mono" value={separator} onChange={(e) => setSeparator(e.target.value)} />
        </label>
        <label className="row">
          <input type="checkbox" checked={regex} onChange={(e) => setRegex(e.target.checked)} />
          {t("dialog.split.regex")}
        </label>
        <label className="row">
          <input type="checkbox" checked={auto} onChange={(e) => setAuto(e.target.checked)} />
          {t("dialog.split.auto")}
        </label>
        {!auto && (
          <label className="row">
            {t("dialog.split.parts")}
            <input type="number" min={1} max={50} className="grow" value={parts} onChange={(e) => setParts(e.target.value)} />
          </label>
        )}
        <label className="row">
          <input type="checkbox" checked={keep} onChange={(e) => setKeep(e.target.checked)} />
          {t("dialog.split.keep_original")}
        </label>
        <div className="pv-hint">{t("dialog.split.hint", { column })}</div>
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openSplitColumn(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <SplitColumnDialog ui={ui} close={close} a={a} />, null);
}
