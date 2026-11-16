import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { ColumnMeta, Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { ColumnChecklist, RichText, SubmitOnEnter } from "./common";
import "./dialogs.css";

interface Args {
  table: string;
  columns: ColumnMeta[];
  selected?: string;
}

/** Join the text of several columns into one (Power Query: Merge Columns). */
function MergeColumnsDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const names = a.columns.map((c) => c.name);
  const [checked, setChecked] = useState<Set<string>>(() => new Set(a.selected ? [a.selected] : []));
  const [separator, setSeparator] = useState(" ");
  const [name, setName] = useState("merged");
  const [keep, setKeep] = useState(true);
  // In table order, as the user sees them.
  const columns = names.filter((n) => checked.has(n));
  const op: Operation | null =
    columns.length >= 2 && name.trim() ? { op: "merge_columns", columns, separator, name: name.trim(), keep_columns: keep } : null;
  const submit = () => op && close(op);

  return (
    <Dialog
      title={t("dialog.merge.title")}
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
        <span>{t("dialog.merge.columns")}</span>
        <ColumnChecklist ui={ui} columns={names} checked={checked} onChange={setChecked} />
        <label className="row">
          {t("dialog.merge.separator")}
          <input type="text" className="grow pv-mono" value={separator} onChange={(e) => setSeparator(e.target.value)} />
        </label>
        <label className="row">
          {t("dialog.merge.name")}
          <input type="text" className="grow" value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <label className="row">
          <input type="checkbox" checked={keep} onChange={(e) => setKeep(e.target.checked)} />
          {t("dialog.merge.keep")}
        </label>
        <div className="pv-hint">{t("dialog.merge.hint")}</div>
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openMergeColumns(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <MergeColumnsDialog ui={ui} close={close} a={a} />, null);
}
