import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import { FILL_METHODS } from "../lib/api";
import type { ColumnMeta, FillMethod, Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { ColumnChecklist, RichText, SubmitOnEnter } from "./common";
import { buildFillNullOp } from "./logic";
import { WhereSection, initialWhere, whereFilter, type Suggest } from "./ConditionRows";
import "./dialogs.css";

interface Args {
  table: string;
  columns: ColumnMeta[];
  selected?: string;
  /** Frequent values of a column, for the "Only rows where…" value boxes. */
  suggest?: Suggest;
}

function FillNullDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const names = a.columns.map((c) => c.name);
  const [checked, setChecked] = useState<Set<string>>(() => new Set(a.selected ? [a.selected] : []));
  const [method, setMethod] = useState<FillMethod>("value");
  const [value, setValue] = useState("");
  const [where, setWhere] = useState(() => initialWhere(a.selected ?? names[0] ?? ""));
  const filter = whereFilter(where, a.columns);
  const base = buildFillNullOp(
    names.filter((n) => checked.has(n)),
    method,
    value,
  );
  const op = base && filter !== null ? (filter ? { ...base, where: filter } : base) : null;
  const submit = () => op && close(op);

  return (
    <Dialog
      title={t("dialog.fill_null.title")}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-medium"
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
        <div>{t("dialog.fill_null.columns")}</div>
        <ColumnChecklist ui={ui} columns={names} checked={checked} onChange={setChecked} />
        <label className="row">
          {t("dialog.fill_null.method")}
          <select className="grow" value={method} onChange={(e) => setMethod(e.target.value as FillMethod)}>
            {FILL_METHODS.map((m) => (
              <option key={m} value={m}>
                {t(`fill.${m}`)}
              </option>
            ))}
          </select>
        </label>
        {method === "value" && (
          <label className="row">
            {t("dialog.fill_null.value")}
            <input type="text" className="grow" value={value} onChange={(e) => setValue(e.target.value)} />
          </label>
        )}
        <div className="pv-hint">{t(`fill.${method}.hint`)}</div>
        <WhereSection t={t} columns={a.columns} value={where} onChange={setWhere} suggest={a.suggest} />
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openFillNull(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <FillNullDialog ui={ui} close={close} a={a} />, null);
}
