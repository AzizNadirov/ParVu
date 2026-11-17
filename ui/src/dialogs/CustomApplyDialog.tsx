import { useEffect, useRef, useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import { api, errorMessage } from "../lib/api";
import type { Cell, ColumnMeta, CustomDefinition, CustomParam, ExampleTable, Operation } from "../lib/api";
import { isCancelled, type Job } from "../lib/jobs";
import { Dialog } from "../components/Dialog";
import { Icon } from "../components/Icon";
import { copyText, SubmitOnEnter, useDebounced } from "./common";
import { WhereSection, initialWhere, whereFilter, type Suggest } from "./ConditionRows";
import {
  buildCustomOp,
  columnsFor,
  customArgProblems,
  customArgs,
  familyOfType,
  fittingColumns,
  initialCustomForm,
  isNewTable,
  signatureText,
  valueKind,
  type CustomForm,
} from "./logic";
import "./dialogs.css";

export interface OtherTable {
  name: string;
  columns: ColumnMeta[];
}

interface Args {
  tabId: number;
  table: string;
  def: CustomDefinition;
  columns: ColumnMeta[];
  /** The column the dialog was opened for. */
  selected?: string;
  /** The other open tables (for table parameters). */
  others: OtherTable[];
  suggest?: Suggest;
}

const INPUT_TYPE: Record<string, string> = { number: "number", date: "date", datetime: "datetime-local", time: "time" };

function ValueField({ p, value, onChange, t }: { p: CustomParam; value: string; onChange(v: string): void; t: Ui["t"] }) {
  const kind = valueKind(familyOfType(p.dtype ?? "VARCHAR"));
  if (p.choices?.length || kind === "boolean") {
    const choices = p.choices?.length ? p.choices : ["true", "false"];
    return (
      <select className="grow" value={value} onChange={(e) => onChange(e.target.value)}>
        {p.optional && <option value="">{t("custom.apply.default")}</option>}
        {choices.map((c) => (
          <option key={c} value={c}>
            {c}
          </option>
        ))}
      </select>
    );
  }
  const type = INPUT_TYPE[kind] ?? "text";
  return (
    <input
      type={type}
      step={type === "number" ? "any" : type === "datetime-local" || type === "time" ? 1 : undefined}
      min={p.min ?? undefined}
      max={p.max ?? undefined}
      className="grow"
      placeholder={p.optional ? (p.default ? t("custom.apply.default_is", { value: p.default }) : t("custom.apply.empty")) : ""}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

/** Apply a custom function or operation: a control per parameter, the
 * output for row functions, a preview; a summary function shows its value. */
function CustomApplyDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const def = a.def;
  const summary = def.kind === "function" && def.result.kind === "value";
  const rowFn = def.kind === "function" && def.result.kind === "column";
  const [form, setForm] = useState<CustomForm>(() => initialCustomForm(def, a.columns, a.selected, a.others));
  const [where, setWhere] = useState(() => initialWhere(form.args[def.params.find((p) => p.kind === "column")?.name ?? ""] ?? ""));
  const filter = rowFn ? whereFilter(where, a.columns) : undefined;
  const problems = customArgProblems(def, a.columns, form, a.others);
  const op = filter === null ? null : buildCustomOp(def, form, a.columns, a.others, filter);
  const set = (name: string, v: string) => setForm((f) => ({ ...f, args: { ...f.args, [name]: v } }));
  const problemOf = (name: string) => problems.find(([p]) => p === name)?.[1];

  const [preview, setPreview] = useState<ExampleTable | null>(null);
  const [previewError, setPreviewError] = useState("");
  const [value, setValue] = useState<{ v: Cell } | null>(null);
  const [running, setRunning] = useState(false);
  const job = useRef<Job<unknown> | null>(null);
  useEffect(() => () => job.current?.cancel(), []);

  useDebounced(op ? JSON.stringify(op) : "", 300, (key) => {
    if (!key) {
      setPreview(null);
      return;
    }
    const j = api.customPreview(a.tabId, JSON.parse(key) as Operation, 12);
    job.current = j;
    j.result
      .then((r) => (setPreview(r), setPreviewError("")))
      .catch((e) => !isCancelled(e) && (setPreview(null), setPreviewError(errorMessage(e))));
    return () => j.cancel();
  });

  const compute = () => {
    if (problems.length) return;
    setRunning(true);
    setValue(null);
    setPreviewError("");
    const j = api.customValue(a.tabId, def.name, customArgs(def, form), { label: def.title || def.name });
    job.current = j;
    j.result
      .then((v) => setValue({ v }))
      .catch((e) => !isCancelled(e) && setPreviewError(errorMessage(e)))
      .finally(() => setRunning(false));
  };

  const submit = () => {
    if (summary) compute();
    else if (op) close(op);
  };
  const tables = def.params.filter((p) => p.kind === "table");

  return (
    <Dialog
      title={def.title || def.name}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-medium pv-custom-apply"
      footer={
        summary ? (
          <>
            <button onClick={() => close(null)}>{t("btn.close")}</button>
            {running ? (
              <button onClick={() => job.current?.cancel()}>{t("custom.btn.stop")}</button>
            ) : (
              <button className="primary" disabled={problems.length > 0} onClick={compute}>
                {t("custom.apply.compute")}
              </button>
            )}
          </>
        ) : (
          <>
            <button onClick={() => close(null)}>{t("btn.cancel")}</button>
            <button className="primary" disabled={!op} onClick={submit}>
              {t("btn.apply")}
            </button>
          </>
        )
      }
    >
      <SubmitOnEnter onSubmit={submit}>
        {def.description && <div className="pv-hint">{def.description}</div>}
        <div className="pv-mono pv-hint pv-selectable">{signatureText(def)}</div>
        {def.kind === "operation" && (
          <div className="pv-hint">{t(isNewTable(def) ? "custom.apply.opens_table" : "custom.apply.changes_table", { table: a.table })}</div>
        )}
        <div className="pv-custom-params">
          {def.params.map((p) => {
            if (p.kind === "table" && p === tables[0]) return null;
            const problem = problemOf(p.name);
            return (
              <label key={p.name} className="row" title={p.description}>
                <span className="pv-custom-param-name">{p.name}</span>
                {p.kind === "table" && (
                  <select className="grow" value={form.args[p.name] ?? ""} onChange={(e) => set(p.name, e.target.value)}>
                    <option value="">—</option>
                    {a.others.map((o) => (
                      <option key={o.name} value={o.name}>
                        {o.name}
                      </option>
                    ))}
                  </select>
                )}
                {p.kind === "column" && (
                  <select className="grow" value={form.args[p.name] ?? ""} onChange={(e) => set(p.name, e.target.value)}>
                    <option value="">—</option>
                    {fittingColumns(p, columnsFor(def, p, a.columns, form.args, a.others)).map((c) => (
                      <option key={c.name} value={c.name}>
                        {c.name}
                      </option>
                    ))}
                  </select>
                )}
                {p.kind === "value" && <ValueField p={p} value={form.args[p.name] ?? ""} onChange={(v) => set(p.name, v)} t={t} />}
                {problem && <span className="error-text small">{t(problem)}</span>}
              </label>
            );
          })}
        </div>
        {rowFn && (
          <div className="row pv-fn-output">
            <label className="row">
              <input type="radio" checked={!form.toNew} onChange={() => setForm((f) => ({ ...f, toNew: false }))} />
              {t("fn.output.same")}
            </label>
            <label className="row">
              <input type="radio" checked={form.toNew} onChange={() => setForm((f) => ({ ...f, toNew: true }))} />
              {t("fn.output.new")}
            </label>
            {form.toNew && (
              <input
                type="text"
                className="grow"
                placeholder={t("fn.output.auto_name")}
                value={form.newName}
                onChange={(e) => setForm((f) => ({ ...f, newName: e.target.value }))}
              />
            )}
          </div>
        )}
        {problemOf("") && <div className="error-text small">{t(problemOf("") ?? "")}</div>}
        {summary && value && (
          <div className="row pv-custom-value">
            <span className="pv-section-title">{t("custom.apply.value")}</span>
            <span className="pv-mono pv-selectable grow">{value.v ?? <span className="muted">{t("grid.null")}</span>}</span>
            {value.v !== null && (
              <button className="icon" title={t("custom.btn.copy")} onClick={() => void copyText(ui, value.v ?? "", t("custom.apply.copied"))}>
                <Icon name="copy" />
              </button>
            )}
          </div>
        )}
        {summary && running && <div className="pv-hint">{t("custom.apply.computing")}</div>}
        {!summary && preview && (
          <div className="col">
            <span className="pv-section-title">{t("custom.apply.preview")}</span>
            <div className="pv-table-scroll pv-custom-preview">
              <table className="simple">
                <thead>
                  <tr>
                    {preview.columns.map((c, i) => (
                      <th key={i}>{c.name}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {preview.rows.map((r, i) => (
                    <tr key={i}>
                      {r.map((v, j) => (
                        <td key={j} className="pv-mono">
                          {v ?? <span className="muted">{t("grid.null")}</span>}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {preview.rows.length === 0 && <span className="pv-hint">{t("custom.apply.no_rows")}</span>}
          </div>
        )}
        {previewError && <div className="error-text small">{previewError}</div>}
        {rowFn && <WhereSection t={t} columns={a.columns} value={where} onChange={setWhere} suggest={a.suggest} />}
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openCustomApply(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <CustomApplyDialog ui={ui} close={close} a={a} />, null);
}
