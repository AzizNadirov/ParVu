import { useMemo, useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { Cell } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { SubmitOnEnter } from "./common";
import { filterUniqueValues, sortUniqueValues } from "./logic";
import "./dialogs.css";

/** Selection keys: NULL cannot be a Set key next to the string "null", so it gets its own. */
const NULL_KEY = "\u0000null";
const keyOf = (v: Cell) => (v === null ? NULL_KEY : v);

function UniqueValuesDialog({ ui, close, column, values }: { ui: Ui; close: Closer<Cell[] | null>; column: string; values: Cell[] }) {
  const t = ui.t;
  const sorted = useMemo(() => sortUniqueValues(values), [values]);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const visible = useMemo(() => filterUniqueValues(sorted, query), [sorted, query]);

  const toggle = (v: Cell) =>
    setSelected((s) => {
      const n = new Set(s);
      const k = keyOf(v);
      if (n.has(k)) n.delete(k);
      else n.add(k);
      return n;
    });
  const selectVisible = () =>
    setSelected((s) => {
      const n = new Set(s);
      visible.forEach((v) => n.add(keyOf(v)));
      return n;
    });
  const submit = () => {
    if (selected.size === 0) return;
    close(sorted.filter((v) => selected.has(keyOf(v))));
  };

  return (
    <Dialog
      title={t("dialog.unique_values", { column_name: column })}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-narrow pv-dialog-tall pv-unique"
      footer={
        <>
          <span className="left muted small">{t("dialog.unique.selected", { count: selected.size })}</span>
          <button onClick={() => close(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={selected.size === 0} onClick={submit}>
            {t("btn.apply_filter")}
          </button>
        </>
      }
    >
      <SubmitOnEnter onSubmit={submit}>
        <div>{t("unique.found", { count: values.length })}</div>
        <input
          type="search"
          placeholder={t("label.search_placeholder")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <div className="row">
          <button onClick={selectVisible}>{t("btn.select_all")}</button>
          <button onClick={() => setSelected(new Set())}>{t("btn.deselect_all")}</button>
        </div>
        <div className="list-box pv-list-fill" role="listbox" aria-multiselectable="true">
          {visible.map((v) => {
            const k = keyOf(v);
            return (
              <div key={k} className={`item ${selected.has(k) ? "selected" : ""}`}>
                <label>
                  <input type="checkbox" checked={selected.has(k)} onChange={() => toggle(v)} />
                  {v === null ? <span className="pv-null-value">NULL</span> : <span className="ellipsis">{v === "" ? " " : v}</span>}
                </label>
              </div>
            );
          })}
          {visible.length === 0 && <div className="item muted">{t("dialog.unique.no_matches")}</div>}
        </div>
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openUniqueValues(ui: Ui, a: { column: string; values: Cell[] }): Promise<Cell[] | null> {
  return ui.showDialog<Cell[] | null>(
    (close) => <UniqueValuesDialog ui={ui} close={close} column={a.column} values={a.values} />,
    null,
  );
}
