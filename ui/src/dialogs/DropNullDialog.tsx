import { useRef, useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { RichText, SubmitOnEnter } from "./common";
import { buildDropNullOp } from "./logic";
import "./dialogs.css";

interface Args {
  table: string;
  columns: string[];
  selected?: string;
}

function DropNullDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const [column, setColumn] = useState(a.selected && a.columns.includes(a.selected) ? a.selected : a.columns[0] ?? "");
  const [useSentinel, setUseSentinel] = useState(false);
  const [value, setValue] = useState("");
  const valueRef = useRef<HTMLInputElement>(null);
  const op = buildDropNullOp(column, useSentinel, value);
  const submit = () => op && close(op);

  return (
    <Dialog
      title={t("dialog.drop_null.title")}
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
        <div>
          <RichText text={t("dialog.drop_null.table", { table: a.table })} />
        </div>
        <label className="row">
          {t("dialog.drop_null.column")}
          <select className="grow" value={column} onChange={(e) => setColumn(e.target.value)}>
            {a.columns.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>
        </label>
        <label className="row">
          <input
            type="checkbox"
            checked={useSentinel}
            onChange={(e) => {
              setUseSentinel(e.target.checked);
              if (e.target.checked) window.setTimeout(() => valueRef.current?.focus(), 0);
            }}
          />
          {t("dialog.drop_null.use_sentinel")}
        </label>
        <label className="row">
          {t("dialog.drop_null.value")}
          <input
            ref={valueRef}
            type="text"
            className="grow"
            disabled={!useSentinel}
            placeholder={t("dialog.drop_null.value_placeholder")}
            value={value}
            onChange={(e) => setValue(e.target.value)}
          />
        </label>
        <div className="pv-hint">{t(useSentinel ? "dialog.drop_null.hint_sentinel" : "dialog.drop_null.hint_null")}</div>
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openDropNull(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <DropNullDialog ui={ui} close={close} a={a} />, null);
}
