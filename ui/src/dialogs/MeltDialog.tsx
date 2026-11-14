import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { ColumnMeta, Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { ColumnChecklist, RichText } from "./common";
import { buildMeltOp } from "./logic";
import "./dialogs.css";

interface Args {
  table: string;
  columns: ColumnMeta[];
  selected?: string;
}

function MeltDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const names = a.columns.map((c) => c.name);
  const [ids, setIds] = useState<Set<string>>(() => new Set(a.selected ? [a.selected] : names.slice(0, 1)));
  const [values, setValues] = useState<Set<string>>(() => new Set());
  const [variable, setVariable] = useState("variable");
  const [value, setValue] = useState("value");
  const op = buildMeltOp(
    names.filter((n) => ids.has(n)),
    names.filter((n) => values.has(n) && !ids.has(n)),
    variable,
    value,
  );
  const submit = () => op && close(op);

  return (
    <Dialog
      title={t("dialog.melt.title")}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-wide"
      footer={
        <>
          <button onClick={() => close(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={!op} onClick={submit}>
            {t("btn.apply")}
          </button>
        </>
      }
    >
      <div>
        <RichText text={t("dialog.common.table", { table: a.table })} />
      </div>
      <div className="pv-two-cols">
        <div className="col">
          <div className="pv-section-title">{t("dialog.melt.ids")}</div>
          <ColumnChecklist ui={ui} columns={names} checked={ids} onChange={setIds} />
        </div>
        <div className="col">
          <div className="pv-section-title">{t("dialog.melt.values")}</div>
          <ColumnChecklist ui={ui} columns={names} checked={values} onChange={setValues} disabled={ids} />
          <div className="pv-hint">{t("dialog.melt.values_hint")}</div>
        </div>
      </div>
      <label className="row">
        {t("dialog.melt.variable_name")}
        <input type="text" className="grow" value={variable} onChange={(e) => setVariable(e.target.value)} />
        {t("dialog.melt.value_name")}
        <input type="text" className="grow" value={value} onChange={(e) => setValue(e.target.value)} />
      </label>
    </Dialog>
  );
}

export function openMelt(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <MeltDialog ui={ui} close={close} a={a} />, null);
}
