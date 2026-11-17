import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { ColumnMeta, Combine, Condition, Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { RichText } from "./common";
import { CombineSelect, ConditionRows, kindOf, type Suggest } from "./ConditionRows";
import { buildFilterOp, emptyCondition, isTextType, opsFor } from "./logic";
import "./dialogs.css";

interface Args {
  table: string;
  columns: ColumnMeta[];
  selected?: string;
  /** Pre-filled first condition, e.g. from a cell's context menu. */
  initial?: Partial<Condition>;
  /** Frequent values of a column, suggested in text value boxes. */
  suggest?: Suggest;
}

function FilterDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const names = a.columns.map((c) => c.name);
  const isText = (name: string) => isTextType(a.columns.find((c) => c.name === name)?.dtype ?? "");
  const [conds, setConds] = useState<Condition[]>(() => {
    const first = { ...emptyCondition(a.selected ?? names[0] ?? ""), ...a.initial };
    // The operator must fit the column's type.
    const ops = opsFor(kindOf(a.columns, first.column));
    return [ops.includes(first.op) ? first : { ...first, op: ops[0] }];
  });
  const [combine, setCombine] = useState<Combine>("and");
  const op = buildFilterOp(conds, combine, isText);
  const submit = () => op && close(op);

  return (
    <Dialog
      title={t("dialog.filter.title")}
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
      <label className="row">
        {t("dialog.filter.keep_rows")}
        <CombineSelect t={t} value={combine} onChange={setCombine} />
      </label>
      <ConditionRows t={t} columns={a.columns} conditions={conds} onChange={setConds} suggest={a.suggest} />
      <div className="pv-hint">{t("dialog.filter.hint")}</div>
    </Dialog>
  );
}

export function openFilter(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <FilterDialog ui={ui} close={close} a={a} />, null);
}
