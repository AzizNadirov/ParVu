import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { Keep, Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { RichText, SubmitOnEnter } from "./common";
import { buildDropDuplicatesOp, buildMarkDuplicatesOp } from "./logic";
import "./dialogs.css";

interface Args {
  table: string;
  columns: string[];
  /** Ticked at the start (the column a menu was opened on). */
  selected?: string;
  /** Mark Duplicates: add a yes/no column instead of dropping rows. */
  mark?: boolean;
}

function DropDuplicatesDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const [checked, setChecked] = useState<Set<string>>(() => new Set(a.selected ? [a.selected] : []));
  const [keep, setKeep] = useState<Keep>("first");
  // A name not taken yet, so a second Mark Duplicates doesn't fail.
  const [name, setName] = useState(() => {
    let n = "is_duplicate";
    for (let i = 2; a.columns.includes(n); i++) n = `is_duplicate_${i}`;
    return n;
  });
  const op = a.mark ? buildMarkDuplicatesOp(a.columns, checked, keep, name) : buildDropDuplicatesOp(a.columns, checked, keep);
  const submit = () => op && close(op);
  const toggle = (c: string) =>
    setChecked((s) => {
      const n = new Set(s);
      if (n.has(c)) n.delete(c);
      else n.add(c);
      return n;
    });

  return (
    <Dialog
      title={t(a.mark ? "dialog.mark_duplicates.title" : "dialog.drop_duplicates.title")}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-narrow"
      footer={
        <>
          <button onClick={() => close(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={a.columns.length === 0 || !op} onClick={submit}>
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
          {t(a.mark ? "dialog.mark_duplicates.mark" : "dialog.drop_duplicates.keep")}
          <select value={keep} onChange={(e) => setKeep(e.target.value as Keep)}>
            {a.mark ? (
              <>
                <option value="first">{t("dialog.mark_duplicates.but_first")}</option>
                <option value="last">{t("dialog.mark_duplicates.but_last")}</option>
                <option value="none">{t("dialog.mark_duplicates.all")}</option>
              </>
            ) : (
              <>
                <option value="first">{t("dialog.drop_duplicates.keep_first")}</option>
                <option value="last">{t("dialog.drop_duplicates.keep_last")}</option>
                <option value="none">{t("dialog.drop_duplicates.keep_none")}</option>
              </>
            )}
          </select>
        </label>
        {a.mark && (
          <label className="row">
            {t("dialog.mark_duplicates.name")}
            <input className="grow" value={name} onChange={(e) => setName(e.target.value)} />
          </label>
        )}
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openDropDuplicates(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <DropDuplicatesDialog ui={ui} close={close} a={a} />, null);
}
