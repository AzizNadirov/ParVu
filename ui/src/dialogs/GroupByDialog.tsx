import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import { AGG_FUNCS } from "../lib/api";
import type { AggFunc, Aggregation, ColumnMeta, Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { ColumnChecklist, ColumnSelect, RichText } from "./common";
import { aggNeedsColumn, buildGroupByOp, isNumericType } from "./logic";
import "./dialogs.css";

interface Args {
  table: string;
  columns: ColumnMeta[];
  selected?: string;
}

function GroupByDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const names = a.columns.map((c) => c.name);
  const [by, setBy] = useState<Set<string>>(() => new Set(a.selected ? [a.selected] : []));
  const firstNumeric = a.columns.find((c) => isNumericType(c.dtype) && !by.has(c.name))?.name ?? "";
  const [aggs, setAggs] = useState<Aggregation[]>(() => [
    { column: "", func: "rows", alias: null },
    ...(firstNumeric ? [{ column: firstNumeric, func: "sum" as AggFunc, alias: null }] : []),
  ]);
  const byList = names.filter((n) => by.has(n));
  const op = buildGroupByOp(byList, aggs);
  const submit = () => op && close(op);
  const update = (i: number, patch: Partial<Aggregation>) => setAggs((l) => l.map((x, j) => (j === i ? { ...x, ...patch } : x)));

  return (
    <Dialog
      title={t("dialog.group_by.title")}
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
          <div className="pv-section-title">{t("dialog.group_by.by")}</div>
          <ColumnChecklist ui={ui} columns={names} checked={by} onChange={setBy} />
          <div className="pv-hint">{t("dialog.group_by.by_hint")}</div>
        </div>
        <div className="col">
          <div className="pv-section-title">{t("dialog.group_by.aggregations")}</div>
          <div className="pv-rules">
            {aggs.map((g, i) => (
              <div key={i} className="row">
                <select value={g.func} onChange={(e) => update(i, { func: e.target.value as AggFunc })}>
                  {AGG_FUNCS.map((f) => (
                    <option key={f} value={f}>
                      {t(`agg.${f}`)}
                    </option>
                  ))}
                </select>
                {aggNeedsColumn(g.func) ? (
                  <ColumnSelect columns={names} value={g.column} onChange={(v) => update(i, { column: v })} emptyLabel="—" />
                ) : (
                  <span className="grow pv-hint">{t("dialog.group_by.all_rows")}</span>
                )}
                <input
                  type="text"
                  className="pv-rule-name"
                  placeholder={t("dialog.group_by.alias")}
                  value={g.alias ?? ""}
                  onChange={(e) => update(i, { alias: e.target.value })}
                />
                <button className="pv-icon-btn" title={t("btn.remove")} onClick={() => setAggs((l) => l.filter((_, j) => j !== i))}>
                  ✕
                </button>
              </div>
            ))}
          </div>
          <div className="row">
            <button onClick={() => setAggs((l) => [...l, { column: firstNumeric, func: "mean", alias: null }])}>
              {t("dialog.group_by.add")}
            </button>
          </div>
        </div>
      </div>
    </Dialog>
  );
}

export function openGroupBy(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <GroupByDialog ui={ui} close={close} a={a} />, null);
}
