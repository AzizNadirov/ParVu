// Set Value: write one value into a column on the rows that match
// conditions (pandas `df.loc[condition, column] = value`).
import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { ColumnMeta, Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { ColumnSelect, RichText, SubmitOnEnter } from "./common";
import { WhereSection, initialWhere, kindOf, whereFilter, type Suggest } from "./ConditionRows";
import { buildSetValueOp } from "./logic";
import "./dialogs.css";

interface Args {
  table: string;
  columns: ColumnMeta[];
  selected?: string;
  suggest?: Suggest;
}

const INPUT_TYPE = { number: "number", date: "date", datetime: "datetime-local", time: "time", text: "text", boolean: "text" } as const;

function SetValueDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const names = a.columns.map((c) => c.name);
  const [column, setColumn] = useState(a.selected ?? names[0] ?? "");
  const [value, setValue] = useState("");
  const [empty, setEmpty] = useState(false);
  const [where, setWhere] = useState(() => initialWhere(a.selected ?? names[0] ?? "", true));
  const kind = kindOf(a.columns, column);
  const filter = whereFilter(where, a.columns) ?? null;
  const op = buildSetValueOp(column, empty ? null : value, kind === "text", filter);
  const submit = () => op && close(op);

  return (
    <Dialog
      title={t("set_value.title")}
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
        <div className="row">
          {t("set_value.set")}
          <ColumnSelect columns={names} value={column} onChange={(c) => (setColumn(c), setValue(""))} className="" />
          {t("set_value.to")}
          {kind === "boolean" ? (
            <select className="grow" disabled={empty} value={value} onChange={(e) => setValue(e.target.value)}>
              <option value="">—</option>
              <option value="true">{t("cond.true")}</option>
              <option value="false">{t("cond.false")}</option>
            </select>
          ) : (
            <input
              type={INPUT_TYPE[kind]}
              step={kind === "number" ? "any" : kind === "datetime" || kind === "time" ? 1 : undefined}
              className="grow"
              disabled={empty}
              value={value}
              onChange={(e) => setValue(e.target.value)}
            />
          )}
          <label className="row">
            <input type="checkbox" checked={empty} onChange={(e) => setEmpty(e.target.checked)} />
            {t("set_value.empty")}
          </label>
        </div>
        <WhereSection t={t} columns={a.columns} value={where} onChange={setWhere} suggest={a.suggest} required />
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openSetValue(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <SetValueDialog ui={ui} close={close} a={a} />, null);
}
