import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import { api } from "../lib/api";
import type { Cell, ColumnMeta, Operation, Side } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { RichText, SubmitOnEnter, useDebounced } from "./common";
import { ADD_UNITS, buildFn, DATE_FORMATS, FN_DIALOGS, initialValues, MATCH_HOWS, type FieldValues, type FnName } from "../lib/colfn";
import { WhereSection, initialWhere, whereFilter, type Suggest } from "./ConditionRows";
import "./dialogs.css";

interface Args {
  tabId: number;
  table: string;
  column: string;
  name: FnName;
  /** A few values of the column from the page, for the preview. */
  values: Cell[];
  /** The table's columns, for "Only rows where…". */
  columns?: ColumnMeta[];
  suggest?: Suggest;
}

/** One column function with parameters (prefix, first N characters, text
 * before a delimiter…), into the same column or a new one, with a preview. */
function ColumnFnDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const spec = FN_DIALOGS[a.name] ?? { fields: [], output: "new" as const };
  const [values, setValues] = useState<FieldValues>(() => initialValues(a.name));
  const [toNew, setToNew] = useState(spec.output === "new");
  const [newName, setNewName] = useState("");
  const [preview, setPreview] = useState<[Cell, Cell][]>([]);
  const [error, setError] = useState("");
  const func = buildFn(a.name, values);
  const set = (k: string, v: string | number | boolean) => setValues((s) => ({ ...s, [k]: v }));

  useDebounced(JSON.stringify(func), 200, (key) => {
    const f = JSON.parse(key);
    if (!f || !a.values.length) {
      setPreview([]);
      return;
    }
    let live = true;
    api
      .previewColumnFn(a.tabId, a.column, f, a.values)
      .then((rows) => live && (setPreview(rows), setError("")))
      .catch((e) => live && (setPreview([]), setError(String((e as { message?: string })?.message ?? e))));
    return () => {
      live = false;
    };
  });

  const [where, setWhere] = useState(() => initialWhere(a.column));
  const filter = a.columns ? whereFilter(where, a.columns) : undefined;
  const op: Operation | null =
    func && filter !== null
      ? {
          op: "column_transform",
          column: a.column,
          func,
          output: toNew ? { to: "new", name: newName.trim() } : { to: "in_place" },
          ...(filter ? { where: filter } : {}),
        }
      : null;
  const submit = () => op && close(op);

  return (
    <Dialog
      title={t(`fn.${a.name}`)}
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
          <RichText text={t("dialog.convert.column", { column: a.column, table: a.table })} />
        </div>
        {spec.fields.map((f) => (
          <label key={f.key} className="row">
            {f.kind !== "bool" && <span>{t(`fn.field.${f.key}`)}</span>}
            {f.kind === "text" && (
              <input
                type="text"
                className="grow pv-mono"
                autoFocus={f === spec.fields[0]}
                value={String(values[f.key])}
                onChange={(e) => set(f.key, e.target.value)}
              />
            )}
            {f.kind === "number" && (
              <input
                type="number"
                className="grow"
                min={f.min}
                autoFocus={f === spec.fields[0]}
                value={String(values[f.key])}
                onChange={(e) => set(f.key, e.target.value)}
              />
            )}
            {f.kind === "bool" && (
              <>
                <input type="checkbox" checked={!!values[f.key]} onChange={(e) => set(f.key, e.target.checked)} />
                {t(`fn.field.${f.key}`)}
              </>
            )}
            {f.kind === "how" && (
              <select className="grow" value={String(values[f.key])} onChange={(e) => set(f.key, e.target.value)}>
                {MATCH_HOWS.map((h) => (
                  <option key={h} value={h}>
                    {t(`fn.how.${h}`)}
                  </option>
                ))}
              </select>
            )}
            {f.kind === "unit" && (
              <select className="grow" value={String(values[f.key])} onChange={(e) => set(f.key, e.target.value)}>
                {ADD_UNITS.map((u) => (
                  <option key={u} value={u}>
                    {t(`unit.${u}`)}
                  </option>
                ))}
              </select>
            )}
            {f.kind === "format" && (
              <>
                <input
                  type="text"
                  className="grow pv-mono"
                  list="pv-date-formats"
                  value={String(values[f.key])}
                  onChange={(e) => set(f.key, e.target.value)}
                />
                <datalist id="pv-date-formats">
                  {DATE_FORMATS.map((d) => (
                    <option key={d} value={d} />
                  ))}
                </datalist>
              </>
            )}
            {f.kind === "side" && (
              <select className="grow" value={String(values[f.key])} onChange={(e) => set(f.key, e.target.value as Side)}>
                <option value="start">{t("fn.side.start")}</option>
                <option value="end">{t("fn.side.end")}</option>
              </select>
            )}
          </label>
        ))}
        <div className="row pv-fn-output">
          <label className="row">
            <input type="radio" checked={!toNew} onChange={() => setToNew(false)} />
            {t("fn.output.same")}
          </label>
          <label className="row">
            <input type="radio" checked={toNew} onChange={() => setToNew(true)} />
            {t("fn.output.new")}
          </label>
          {toNew && (
            <input
              type="text"
              className="grow"
              placeholder={t("fn.output.auto_name")}
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
            />
          )}
        </div>
        {preview.length > 0 && (
          <div className="pv-table-scroll">
            <table className="simple">
              <thead>
                <tr>
                  <th>{t("dialog.convert.before")}</th>
                  <th>{t("fn.after_value")}</th>
                </tr>
              </thead>
              <tbody>
                {preview.map(([before, after], i) => (
                  <tr key={i}>
                    <td className="pv-mono">{before}</td>
                    <td className="pv-mono">{after ?? <span className="muted">{t("grid.null")}</span>}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {error && <div className="error-text small">{error}</div>}
        {a.columns && <WhereSection t={t} columns={a.columns} value={where} onChange={setWhere} suggest={a.suggest} />}
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openColumnFn(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <ColumnFnDialog ui={ui} close={close} a={a} />, null);
}
