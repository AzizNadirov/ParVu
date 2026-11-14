import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import { AGG_FUNCS } from "../lib/api";
import type { AggFunc, ColumnMeta, Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { ColumnChecklist, ColumnSelect, RichText, SubmitOnEnter } from "./common";
import { buildPivotOp, isNumericType } from "./logic";
import "./dialogs.css";

interface Args {
  table: string;
  columns: ColumnMeta[];
  selected?: string;
}

function PivotDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const names = a.columns.map((c) => c.name);
  const [index, setIndex] = useState<Set<string>>(() => new Set());
  const [columns, setColumns] = useState(a.selected ?? names[0] ?? "");
  const [values, setValues] = useState(() => a.columns.find((c) => isNumericType(c.dtype))?.name ?? "");
  const [func, setFunc] = useState<AggFunc>("sum");
  const indexList = names.filter((n) => index.has(n));
  const op = buildPivotOp(indexList, columns, values, values ? func : "rows");
  const submit = () => op && close(op);

  return (
    <Dialog
      title={t("dialog.pivot.title")}
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
        <div className="pv-section-title">{t("dialog.pivot.rows")}</div>
        <ColumnChecklist ui={ui} columns={names} checked={index} onChange={setIndex} disabled={new Set([columns])} />
        <label className="row">
          {t("dialog.pivot.columns")}
          <ColumnSelect columns={names} value={columns} onChange={setColumns} />
        </label>
        <label className="row">
          {t("dialog.pivot.values")}
          <ColumnSelect columns={names} value={values} onChange={setValues} emptyLabel={t("dialog.pivot.count_rows")} />
          <select value={func} disabled={!values} onChange={(e) => setFunc(e.target.value as AggFunc)}>
            {AGG_FUNCS.filter((f) => f !== "rows").map((f) => (
              <option key={f} value={f}>
                {t(`agg.${f}`)}
              </option>
            ))}
          </select>
        </label>
        <div className="pv-hint">{t("dialog.pivot.hint")}</div>
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openPivot(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <PivotDialog ui={ui} close={close} a={a} />, null);
}
