import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { Operation, TimeUnitName } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { ColumnSelect, RichText, SubmitOnEnter } from "./common";
import "./dialogs.css";

const DIFF_UNITS: TimeUnitName[] = ["day", "week", "month", "quarter", "year", "hour", "minute"];
const LAST_UNITS: TimeUnitName[] = ["day", "week", "month", "quarter", "year"];

interface DiffArgs {
  table: string;
  /** Date columns of the table. */
  dates: string[];
  /** The column the menu was opened on: the end date. */
  end: string;
}

/** Whole days (months, years…) between two date columns, as a new column. */
function DateDiffDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: DiffArgs }) {
  const t = ui.t;
  const others = a.dates.filter((d) => d !== a.end);
  const [start, setStart] = useState(others[0] ?? "");
  const [unit, setUnit] = useState<TimeUnitName>("day");
  const [name, setName] = useState("");
  const finalName = name.trim() || `${a.end}_${unit}s`;
  const op: Operation | null = start ? { op: "date_diff", start, end: a.end, unit, name: finalName } : null;
  const submit = () => op && close(op);
  return (
    <Dialog
      title={t("dates.diff.title")}
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
        <RichText text={t("dates.diff.intro", { end: a.end, table: a.table })} />
        {others.length === 0 ? (
          <div className="muted">{t("dates.diff.no_other")}</div>
        ) : (
          <label className="row">
            {t("dates.diff.from")}
            <ColumnSelect columns={others} value={start} onChange={setStart} />
          </label>
        )}
        <label className="row">
          {t("dates.diff.in")}
          <select className="grow" value={unit} onChange={(e) => setUnit(e.target.value as TimeUnitName)}>
            {DIFF_UNITS.map((u) => (
              <option key={u} value={u}>
                {t(`unit.${u}s`)}
              </option>
            ))}
          </select>
        </label>
        <label className="row">
          {t("dialog.merge.name")}
          <input type="text" className="grow" placeholder={finalName} value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <div className="pv-hint">{t("dates.diff.hint")}</div>
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openDateDiff(ui: Ui, a: DiffArgs): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <DateDiffDialog ui={ui} close={close} a={a} />, null);
}

/** Keep the last N days (weeks, months…) up to today. */
function LastNDialog({ ui, close, column }: { ui: Ui; close: Closer<Operation | null>; column: string }) {
  const t = ui.t;
  const [n, setN] = useState("7");
  const [unit, setUnit] = useState<TimeUnitName>("day");
  const count = Number(n);
  const op: Operation | null =
    Number.isInteger(count) && count > 0 ? { op: "date_range", column, range: { kind: "last", n: count, unit } } : null;
  const submit = () => op && close(op);
  return (
    <Dialog
      title={t("dates.last_n.title")}
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
        <label className="row">
          {t("dates.last_n.keep")}
          <input type="number" min={1} style={{ width: 90 }} autoFocus value={n} onChange={(e) => setN(e.target.value)} />
          <select value={unit} onChange={(e) => setUnit(e.target.value as TimeUnitName)}>
            {LAST_UNITS.map((u) => (
              <option key={u} value={u}>
                {t(`unit.${u}s`)}
              </option>
            ))}
          </select>
        </label>
        <div className="pv-hint">{t("dates.last_n.hint", { column })}</div>
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openLastN(ui: Ui, column: string): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <LastNDialog ui={ui} close={close} column={column} />, null);
}
