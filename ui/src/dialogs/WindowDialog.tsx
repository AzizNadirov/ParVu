import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import { WINDOW_FUNCS } from "../lib/api";
import type { ColumnMeta, Operation, RankMethod, WindowFunc } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { ColumnChecklist, ColumnSelect, RichText, SubmitOnEnter } from "./common";
import { buildWindowOp, defaultWindowOutput, isNumericType, windowNeedsColumn, windowUses } from "./logic";
import "./dialogs.css";

interface Args {
  table: string;
  columns: ColumnMeta[];
  selected?: string;
  func?: WindowFunc;
}

/** Cumulative, shift/diff, rank and rolling columns (pandas cumsum, shift, rank, rolling…). */
function WindowDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const names = a.columns.map((c) => c.name);
  const [func, setFunc] = useState<WindowFunc>(a.func ?? "cum_sum");
  const [column, setColumn] = useState(a.selected ?? a.columns.find((c) => isNumericType(c.dtype))?.name ?? names[0] ?? "");
  const [periods, setPeriods] = useState("1");
  const [windowRows, setWindowRows] = useState("3");
  const [rankMethod, setRankMethod] = useState<RankMethod>("average");
  const [ascending, setAscending] = useState(true);
  const [partition, setPartition] = useState<Set<string>>(() => new Set());
  const [orderBy, setOrderBy] = useState("");
  const [output, setOutput] = useState<string | null>(null);
  const out = output ?? defaultWindowOutput(func, column);
  const uses = windowUses(func);
  const op = buildWindowOp({
    column,
    func,
    periods: Number(periods),
    window: Number(windowRows),
    rank_method: rankMethod,
    ascending,
    partition_by: names.filter((n) => partition.has(n)),
    order_by: orderBy || null,
    output: out,
  });
  const submit = () => op && close(op);

  return (
    <Dialog
      title={t("dialog.window.title")}
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
      <SubmitOnEnter onSubmit={submit}>
        <div>
          <RichText text={t("dialog.common.table", { table: a.table })} />
        </div>
        <div className="pv-two-cols">
          <div className="col">
            <label className="row">
              {t("dialog.window.function")}
              <select className="grow" value={func} onChange={(e) => setFunc(e.target.value as WindowFunc)}>
                {WINDOW_FUNCS.map((f) => (
                  <option key={f} value={f}>
                    {t(`window.${f}`)}
                  </option>
                ))}
              </select>
            </label>
            <div className="pv-hint">{t(`window.${func}.hint`)}</div>
            {windowNeedsColumn(func) && (
              <label className="row">
                {t("dialog.common.column")}
                <ColumnSelect columns={names} value={column} onChange={setColumn} />
              </label>
            )}
            {uses.periods && (
              <label className="row">
                {t("dialog.window.periods")}
                <input type="number" className="grow" value={periods} onChange={(e) => setPeriods(e.target.value)} />
              </label>
            )}
            {uses.window && (
              <label className="row">
                {t("dialog.window.window")}
                <input type="number" min={1} className="grow" value={windowRows} onChange={(e) => setWindowRows(e.target.value)} />
              </label>
            )}
            {uses.rank && func === "rank" && (
              <label className="row">
                {t("dialog.window.ties")}
                <select className="grow" value={rankMethod} onChange={(e) => setRankMethod(e.target.value as RankMethod)}>
                  {(["average", "min", "max", "dense", "first"] as const).map((m) => (
                    <option key={m} value={m}>
                      {t(`rank.${m}`)}
                    </option>
                  ))}
                </select>
              </label>
            )}
            {(uses.rank || orderBy) && (
              <label className="row">
                {t("dialog.window.direction")}
                <select className="grow" value={ascending ? "asc" : "desc"} onChange={(e) => setAscending(e.target.value === "asc")}>
                  <option value="asc">{t("dialog.sort.ascending")}</option>
                  <option value="desc">{t("dialog.sort.descending")}</option>
                </select>
              </label>
            )}
            {!uses.rank && (
              <label className="row">
                {t("dialog.window.order_by")}
                <ColumnSelect columns={names} value={orderBy} onChange={setOrderBy} emptyLabel={t("dialog.window.table_order")} />
              </label>
            )}
            <label className="row">
              {t("dialog.common.output")}
              <input type="text" className="grow" value={out} onChange={(e) => setOutput(e.target.value)} />
            </label>
          </div>
          <div className="col">
            <div className="pv-section-title">{t("dialog.window.partition")}</div>
            <ColumnChecklist ui={ui} columns={names} checked={partition} onChange={setPartition} />
            <div className="pv-hint">{t("dialog.window.partition_hint")}</div>
          </div>
        </div>
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openWindow(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <WindowDialog ui={ui} close={close} a={a} />, null);
}
