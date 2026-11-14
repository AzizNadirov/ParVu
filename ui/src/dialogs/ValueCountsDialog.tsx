import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { ColumnMeta, Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { ColumnChecklist, RichText, SubmitOnEnter } from "./common";
import { buildValueCountsOp } from "./logic";
import "./dialogs.css";

interface Args {
  table: string;
  columns: ColumnMeta[];
  selected?: string;
}

function ValueCountsDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const names = a.columns.map((c) => c.name);
  const [checked, setChecked] = useState<Set<string>>(() => new Set(a.selected ? [a.selected] : names.slice(0, 1)));
  const [normalize, setNormalize] = useState(true);
  const [dropna, setDropna] = useState(false);
  const op = buildValueCountsOp(
    names.filter((n) => checked.has(n)),
    normalize,
    dropna,
  );
  const submit = () => op && close(op);

  return (
    <Dialog
      title={t("dialog.value_counts.title")}
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
        <div>{t("dialog.value_counts.columns")}</div>
        <ColumnChecklist ui={ui} columns={names} checked={checked} onChange={setChecked} />
        <label className="row">
          <input type="checkbox" checked={normalize} onChange={(e) => setNormalize(e.target.checked)} />
          {t("dialog.value_counts.normalize")}
        </label>
        <label className="row">
          <input type="checkbox" checked={dropna} onChange={(e) => setDropna(e.target.checked)} />
          {t("dialog.value_counts.dropna")}
        </label>
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openValueCounts(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <ValueCountsDialog ui={ui} close={close} a={a} />, null);
}
