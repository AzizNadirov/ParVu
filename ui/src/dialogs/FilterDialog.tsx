import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import { COND_OPS } from "../lib/api";
import type { ColumnMeta, Combine, CondOp, Condition, Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { ColumnSelect, RichText } from "./common";
import { buildFilterOp, condArity, condIsTextual, emptyCondition, isTextType } from "./logic";
import "./dialogs.css";
import { Icon } from "../components/Icon";

interface Args {
  table: string;
  columns: ColumnMeta[];
  selected?: string;
  /** Pre-filled first condition, e.g. from a cell's context menu. */
  initial?: Partial<Condition>;
}

function FilterDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const names = a.columns.map((c) => c.name);
  const isText = (name: string) => isTextType(a.columns.find((c) => c.name === name)?.dtype ?? "");
  const [conds, setConds] = useState<Condition[]>(() => [
    { ...emptyCondition(a.selected ?? names[0] ?? ""), ...a.initial },
  ]);
  const [combine, setCombine] = useState<Combine>("and");
  const op = buildFilterOp(conds, combine, isText);
  const submit = () => op && close(op);
  const update = (i: number, patch: Partial<Condition>) => setConds((l) => l.map((x, j) => (j === i ? { ...x, ...patch } : x)));

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
        <select value={combine} onChange={(e) => setCombine(e.target.value as Combine)}>
          <option value="and">{t("dialog.filter.all")}</option>
          <option value="or">{t("dialog.filter.any")}</option>
        </select>
      </label>
      <div className="pv-rules">
        {conds.map((c, i) => {
          const arity = condArity(c.op);
          return (
            <div key={i} className="row">
              <ColumnSelect columns={names} value={c.column} onChange={(v) => update(i, { column: v })} className="" />
              <select value={c.op} onChange={(e) => update(i, { op: e.target.value as CondOp })}>
                {COND_OPS.map((o) => (
                  <option key={o} value={o}>
                    {t(`cond.${o}`)}
                  </option>
                ))}
              </select>
              {arity >= 1 && (
                <input
                  type="text"
                  className="grow"
                  placeholder={c.op === "in" || c.op === "not_in" ? t("dialog.filter.list_placeholder") : t("dialog.filter.value")}
                  value={c.value}
                  onChange={(e) => update(i, { value: e.target.value })}
                />
              )}
              {arity === 2 && (
                <>
                  {t("dialog.filter.and")}
                  <input type="text" className="grow" value={c.value2} onChange={(e) => update(i, { value2: e.target.value })} />
                </>
              )}
              {arity === 0 && <span className="grow" />}
              <label className="row" title={t("dialog.filter.case_sensitive")}>
                <input
                  type="checkbox"
                  disabled={!condIsTextual(c.op)}
                  checked={c.case_sensitive}
                  onChange={(e) => update(i, { case_sensitive: e.target.checked })}
                />
                Aa
              </label>
              <button
                className="pv-icon-btn"
                title={t("btn.remove")}
                disabled={conds.length === 1}
                onClick={() => setConds((l) => l.filter((_, j) => j !== i))}
              >
                <Icon name="close" />
              </button>
            </div>
          );
        })}
      </div>
      <div className="row">
        <button onClick={() => setConds((l) => [...l, emptyCondition(l[l.length - 1]?.column ?? names[0] ?? "")])}>
          {t("dialog.filter.add")}
        </button>
      </div>
      <div className="pv-hint">{t("dialog.filter.hint")}</div>
    </Dialog>
  );
}

export function openFilter(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <FilterDialog ui={ui} close={close} a={a} />, null);
}
