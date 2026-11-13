import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { JoinType, Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { RichText, SubmitOnEnter } from "./common";
import { JOIN_TYPES, buildJoinOp } from "./logic";
import "./dialogs.css";

interface Args {
  table: string;
  columns: string[];
  others: { name: string; columns: string[] }[];
}

/** Right key defaults to the column with the same name as the left key, if any. */
export function pickRightKey(leftKey: string, rightColumns: string[]): string {
  return rightColumns.includes(leftKey) ? leftKey : rightColumns[0] ?? "";
}

function JoinDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const [right, setRight] = useState(a.others[0]?.name ?? "");
  const [joinType, setJoinType] = useState<JoinType>("inner");
  const [leftKey, setLeftKey] = useState(a.columns[0] ?? "");
  const rightCols = a.others.find((o) => o.name === right)?.columns ?? [];
  const [rightKey, setRightKey] = useState(() => pickRightKey(a.columns[0] ?? "", a.others[0]?.columns ?? []));

  const op = buildJoinOp(right, joinType, leftKey, rightKey);
  const submit = () => op && close(op);

  return (
    <Dialog
      title={t("dialog.join.title")}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-narrow"
      footer={
        <>
          <button onClick={() => close(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={!op} onClick={submit}>
            {t("btn.ok")}
          </button>
        </>
      }
    >
      <SubmitOnEnter onSubmit={submit}>
        <div>
          <RichText text={t("dialog.join.left_table", { table: a.table })} />
        </div>
        {a.others.length === 0 ? (
          <div className="warn-text">{t("dialog.join.no_tables")}</div>
        ) : (
          <div className="form-grid">
            <label htmlFor="pv-join-right">{t("dialog.join.right_table")}</label>
            <select
              id="pv-join-right"
              value={right}
              onChange={(e) => {
                const name = e.target.value;
                setRight(name);
                setRightKey(pickRightKey(leftKey, a.others.find((o) => o.name === name)?.columns ?? []));
              }}
            >
              {a.others.map((o) => (
                <option key={o.name} value={o.name}>
                  {o.name}
                </option>
              ))}
            </select>
            <label htmlFor="pv-join-type">{t("dialog.join.type")}</label>
            <select id="pv-join-type" value={joinType} onChange={(e) => setJoinType(e.target.value as JoinType)}>
              {JOIN_TYPES.map((j) => (
                <option key={j.value} value={j.value}>
                  {j.label}
                </option>
              ))}
            </select>
            <label htmlFor="pv-join-left-key">{t("dialog.join.left_key")}</label>
            <select id="pv-join-left-key" value={leftKey} onChange={(e) => setLeftKey(e.target.value)}>
              {a.columns.map((c) => (
                <option key={c} value={c}>
                  {c}
                </option>
              ))}
            </select>
            <label htmlFor="pv-join-right-key">{t("dialog.join.right_key")}</label>
            <select id="pv-join-right-key" value={rightKey} onChange={(e) => setRightKey(e.target.value)}>
              {rightCols.map((c) => (
                <option key={c} value={c}>
                  {c}
                </option>
              ))}
            </select>
          </div>
        )}
        {op && (
          <div className="pv-sql-preview small">
            {`SELECT * FROM ${a.table}\n${JOIN_TYPES.find((j) => j.value === joinType)?.label} JOIN ${right}\nON ${a.table}.${leftKey} = ${right}.${rightKey}`}
          </div>
        )}
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openJoinDialog(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <JoinDialog ui={ui} close={close} a={a} />, null);
}
