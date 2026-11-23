import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import { AGG_FUNCS } from "../lib/api";
import type { AggFunc, Aggregation, ColumnMeta, CustomDefinition, Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { ColumnChecklist, ColumnSelect, RichText } from "./common";
import { aggChoice, aggNeedsColumn, buildGroupByOp, fittingColumns, isNumericType, withAggChoice } from "./logic";
import "./dialogs.css";
import { Icon } from "../components/Icon";

interface Args {
  table: string;
  columns: ColumnMeta[];
  selected?: string;
  /** Summary custom functions that can aggregate (one column parameter). */
  custom?: CustomDefinition[];
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
  const customDefs = a.custom ?? [];
  const defOf = (g: Aggregation) => (g.func === "custom" ? customDefs.find((d) => d.name === g.custom?.name) : undefined);

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
              <div key={i} className="pv-agg-row">
                <select
                  value={aggChoice(g)}
                  onChange={(e) => setAggs((l) => l.map((x, j) => (j === i ? withAggChoice(x, e.target.value) : x)))}
                >
                  {AGG_FUNCS.map((f) => (
                    <option key={f} value={f}>
                      {t(`agg.${f}`)}
                    </option>
                  ))}
                  {customDefs.length > 0 && (
                    <optgroup label={t("dialog.group_by.custom")}>
                      {customDefs.map((d) => (
                        <option key={d.name} value={`custom:${d.name}`} title={d.description}>
                          {d.title || d.name}
                        </option>
                      ))}
                    </optgroup>
                  )}
                </select>
                {aggNeedsColumn(g.func) ? (
                  <ColumnSelect
                    columns={(() => {
                      const d = defOf(g);
                      const p = d?.params.find((x) => x.kind === "column");
                      return p ? fittingColumns(p, a.columns).map((c) => c.name) : names;
                    })()}
                    value={g.column}
                    onChange={(v) => update(i, { column: v })}
                    emptyLabel="—"
                  />
                ) : (
                  <span className="grow pv-hint">{t("dialog.group_by.all_rows")}</span>
                )}
                {defOf(g)
                  ?.params.filter((p) => p.kind === "value")
                  .map((p) => (
                    <input
                      key={p.name}
                      type="text"
                      className="pv-rule-name"
                      title={p.description || p.name}
                      placeholder={p.default !== undefined && p.default !== null ? `${p.name} = ${p.default}` : p.name}
                      value={g.custom?.values?.[p.name] ?? ""}
                      onChange={(e) =>
                        update(i, { custom: { name: g.custom?.name ?? "", values: { ...(g.custom?.values ?? {}), [p.name]: e.target.value } } })
                      }
                    />
                  ))}
                <input
                  type="text"
                  className="pv-rule-name pv-agg-alias"
                  placeholder={t("dialog.group_by.alias")}
                  value={g.alias ?? ""}
                  onChange={(e) => update(i, { alias: e.target.value })}
                />
                <button
                  className="pv-icon-btn pv-agg-remove"
                  title={t("btn.remove")}
                  aria-label={t("btn.remove")}
                  onClick={() => setAggs((l) => l.filter((_, j) => j !== i))}
                >
                  <Icon name="close" />
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
