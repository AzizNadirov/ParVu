import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { Keep, Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { RichText, SubmitOnEnter } from "./common";
import { buildDropDuplicatesOp } from "./logic";
import "./dialogs.css";

interface Args {
  table: string;
  columns: string[];
}

function DropDuplicatesDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const [checked, setChecked] = useState<Set<string>>(() => new Set());
  const [keep, setKeep] = useState<Keep>("first");
  const submit = () => close(buildDropDuplicatesOp(a.columns, checked, keep));
  const toggle = (c: string) =>
    setChecked((s) => {
      const n = new Set(s);
      if (n.has(c)) n.delete(c);
      else n.add(c);
      return n;
    });

  return (
    <Dialog
      title={t("dialog.drop_duplicates.title")}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-narrow"
      footer={
        <>
          <button onClick={() => close(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={a.columns.length === 0} onClick={submit}>
            {t("btn.ok")}
          </button>
        </>
      }
    >
      <SubmitOnEnter onSubmit={submit}>
        <div>
          <RichText text={t("dialog.drop_duplicates.table", { table: a.table })} />
        </div>
        <div>{t("dialog.drop_duplicates.select")}</div>
        <div className="pv-hint">{t("dialog.drop_duplicates.hint")}</div>
        <div className="row">
          <button onClick={() => setChecked(new Set(a.columns))}>{t("btn.select_all")}</button>
          <button onClick={() => setChecked(new Set())}>{t("btn.deselect_all")}</button>
        </div>
        <div className="list-box">
          {a.columns.map((c) => (
            <div key={c} className={`item ${checked.has(c) ? "selected" : ""}`}>
              <label className="row grow">
                <input type="checkbox" checked={checked.has(c)} onChange={() => toggle(c)} />
                {c}
              </label>
            </div>
          ))}
        </div>
        <label className="row">
          {t("dialog.drop_duplicates.keep")}
          <select value={keep} onChange={(e) => setKeep(e.target.value as Keep)}>
            <option value="first">{t("dialog.drop_duplicates.keep_first")}</option>
            <option value="last">{t("dialog.drop_duplicates.keep_last")}</option>
          </select>
        </label>
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openDropDuplicates(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <DropDuplicatesDialog ui={ui} close={close} a={a} />, null);
}
