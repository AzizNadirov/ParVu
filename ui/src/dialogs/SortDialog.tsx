import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { ColumnMeta, Operation, SortKey } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { ColumnSelect, RichText } from "./common";
import { buildSortByOp, moveItem } from "./logic";
import "./dialogs.css";

interface Args {
  table: string;
  columns: ColumnMeta[];
  selected?: string;
}

function SortDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const names = a.columns.map((c) => c.name);
  const [keys, setKeys] = useState<SortKey[]>(() => [
    { column: a.selected ?? names[0] ?? "", ascending: true, nulls_first: false },
  ]);
  const op = buildSortByOp(keys);
  const submit = () => op && close(op);
  const update = (i: number, patch: Partial<SortKey>) => setKeys((l) => l.map((x, j) => (j === i ? { ...x, ...patch } : x)));
  const unused = names.find((n) => !keys.some((k) => k.column === n));

  return (
    <Dialog
      title={t("dialog.sort.title")}
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
      <div>
        <RichText text={t("dialog.common.table", { table: a.table })} />
      </div>
      <div className="pv-rules">
        {keys.map((k, i) => (
          <div key={i} className="row">
            <span className="pv-hint">{i === 0 ? t("dialog.sort.by") : t("dialog.sort.then")}</span>
            <ColumnSelect columns={names} value={k.column} onChange={(v) => update(i, { column: v })} />
            <select value={k.ascending ? "asc" : "desc"} onChange={(e) => update(i, { ascending: e.target.value === "asc" })}>
              <option value="asc">{t("dialog.sort.ascending")}</option>
              <option value="desc">{t("dialog.sort.descending")}</option>
            </select>
            <label className="row">
              <input type="checkbox" checked={k.nulls_first} onChange={(e) => update(i, { nulls_first: e.target.checked })} />
              {t("dialog.sort.nulls_first")}
            </label>
            <button className="pv-icon-btn" disabled={i === 0} onClick={() => setKeys((l) => moveItem(l, i, -1))}>
              ▲
            </button>
            <button className="pv-icon-btn" disabled={i === keys.length - 1} onClick={() => setKeys((l) => moveItem(l, i, 1))}>
              ▼
            </button>
            <button
              className="pv-icon-btn"
              title={t("btn.remove")}
              disabled={keys.length === 1}
              onClick={() => setKeys((l) => l.filter((_, j) => j !== i))}
            >
              ✕
            </button>
          </div>
        ))}
      </div>
      <div className="row">
        <button
          disabled={!unused}
          onClick={() => unused && setKeys((l) => [...l, { column: unused, ascending: true, nulls_first: false }])}
        >
          {t("dialog.sort.add")}
        </button>
      </div>
      {!op && keys.length > 1 && <div className="pv-hint">{t("dialog.sort.duplicate")}</div>}
    </Dialog>
  );
}

export function openSort(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <SortDialog ui={ui} close={close} a={a} />, null);
}
