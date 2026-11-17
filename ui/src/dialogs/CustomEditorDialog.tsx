import { useMemo, useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import { api, errorMessage } from "../lib/api";
import type { CustomDefinition, CustomExample, CustomParam, CustomReport, ExampleOutcome, ExampleTable } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { Icon } from "../components/Icon";
import { QueryEditor } from "../components/QueryEditor";
import {
  EXAMPLE_TYPES,
  cellFromInput,
  cellToInput,
  customCheckText,
  definitionToSave,
  expectedFromActual,
  formatRequiredColumns,
  newExample,
  parseRequiredColumns,
  signatureText,
  syncFunctionExample,
} from "./logic";
import "./dialogs.css";

interface Args {
  definition: CustomDefinition;
  /** The name it is saved under (null: a new definition). */
  editing: string | null;
  typeSets: string[];
  valueTypes: string[];
}

const reportPassed = (r: CustomReport | null) => !!r && !!r.compiled && !r.checks.some((c) => c.status === "fail");

const STATUS_ICON = { pass: "checkCircle", warning: "warning", fail: "failCircle", skipped: "skipCircle" } as const;

/** An editable grid of example cells. Empty cells are "no value"; `""` is empty text. */
function Grid(props: {
  t: Ui["t"];
  table: ExampleTable;
  onChange?(t: ExampleTable): void;
  /** Column names and types can be changed (operations). */
  editColumns?: boolean;
  /** Show the type row. */
  types?: boolean;
  /** Cells to mark, as `[row, column]`. */
  marks?: [number, number][];
  /** Actual values, shown on marked cells. */
  actual?: ExampleTable;
  /** Extra cells on the right of each row (a row function's expected value). */
  extra?: { header: string; cell(row: number): React.ReactNode };
  fixedRows?: boolean;
}) {
  const { t, table, onChange } = props;
  const edit = (fn: (x: ExampleTable) => void) => {
    if (!onChange) return;
    const copy: ExampleTable = JSON.parse(JSON.stringify(table));
    fn(copy);
    onChange(copy);
  };
  const marked = (r: number, c: number) => props.marks?.some(([mr, mc]) => mr === r && (mc === c || mc === -1));
  return (
    <div className="pv-table-scroll pv-custom-grid">
      <table className="simple">
        <thead>
          <tr>
            {table.columns.map((c, i) => (
              <th key={i}>
                {props.editColumns ? (
                  <span className="row">
                    <input
                      type="text"
                      className="grow pv-mono"
                      value={c.name}
                      aria-label={t("custom.editor.column_name")}
                      onChange={(e) => edit((x) => (x.columns[i].name = e.target.value))}
                    />
                    {table.columns.length > 1 && (
                      <button
                        className="icon"
                        title={t("custom.editor.remove_column")}
                        onClick={() =>
                          edit((x) => {
                            x.columns.splice(i, 1);
                            x.rows.forEach((r) => r.splice(i, 1));
                          })
                        }
                      >
                        <Icon name="close" size={14} />
                      </button>
                    )}
                  </span>
                ) : (
                  <span className="pv-mono">{c.name}</span>
                )}
              </th>
            ))}
            {props.extra && <th>{props.extra.header}</th>}
            {onChange && !props.fixedRows && <th />}
          </tr>
          {props.types && (
            <tr>
              {table.columns.map((c, i) => (
                <th key={i}>
                  <select value={c.dtype ?? "VARCHAR"} disabled={!onChange} onChange={(e) => edit((x) => (x.columns[i].dtype = e.target.value))}>
                    {[...new Set([...EXAMPLE_TYPES, c.dtype ?? "VARCHAR"])].map((ty) => (
                      <option key={ty} value={ty}>
                        {ty}
                      </option>
                    ))}
                  </select>
                </th>
              ))}
              {props.extra && <th />}
              {onChange && !props.fixedRows && <th />}
            </tr>
          )}
        </thead>
        <tbody>
          {table.rows.map((row, r) => (
            <tr key={r}>
              {table.columns.map((_, c) => {
                const bad = marked(r, c);
                const actual = bad ? props.actual?.rows[r]?.[c] : undefined;
                return (
                  <td key={c} className={bad ? "pv-custom-bad" : undefined} title={bad && actual !== undefined ? t("custom.editor.actual", { value: actual ?? "∅" }) : undefined}>
                    <input
                      type="text"
                      className="pv-mono"
                      placeholder="∅"
                      readOnly={!onChange}
                      value={cellToInput(row[c] ?? null)}
                      onChange={(e) => edit((x) => (x.rows[r][c] = cellFromInput(e.target.value)))}
                    />
                  </td>
                );
              })}
              {props.extra && <td>{props.extra.cell(r)}</td>}
              {onChange && !props.fixedRows && (
                <td>
                  <button className="icon" title={t("custom.editor.remove_row")} onClick={() => edit((x) => x.rows.splice(r, 1))}>
                    <Icon name="close" size={14} />
                  </button>
                </td>
              )}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ExampleEditor(props: {
  t: Ui["t"];
  def: CustomDefinition;
  ex: CustomExample;
  outcome?: ExampleOutcome;
  onChange(ex: CustomExample): void;
  onFill(): void;
  busy: boolean;
}) {
  const { t, def, ex, outcome, onChange } = props;
  const fn = def.kind === "function";
  const summary = fn && def.result.kind === "value";
  const values = def.params.filter((p) => p.kind === "value");
  const columnParams = def.params.filter((p) => p.kind === "column");
  const otherTables = def.params.filter((p) => p.kind === "table").slice(1);
  const setArg = (name: string, v: string | null) => onChange({ ...ex, args: { ...(ex.args ?? {}), [name]: v } });
  const setExpectedCell = (r: number, v: string) => {
    const rows = ex.expected.rows.map((x) => [...x]);
    while (rows.length <= r) rows.push([null]);
    rows[r][0] = cellFromInput(v);
    onChange({ ...ex, expected: { columns: [{ name: "result" }], rows } });
  };
  const marks = outcome?.mismatches ?? [];
  const expectedBad = (r: number) => marks.some(([mr]) => mr === r);
  const addRow = (table: ExampleTable): ExampleTable => ({ ...table, rows: [...table.rows, table.columns.map(() => null)] });
  const addColumn = (table: ExampleTable, dtype = true): ExampleTable => {
    const name = `c${table.columns.length + 1}`;
    return { columns: [...table.columns, dtype ? { name, dtype: "VARCHAR" } : { name }], rows: table.rows.map((r) => [...r, null]) };
  };
  const inputColumns = ex.input.columns.map((c) => c.name);

  return (
    <div className="col pv-custom-example">
      <div className="row">
        <label className="row grow">
          <span>{t("custom.editor.example_name")}</span>
          <input type="text" className="grow" value={ex.name} onChange={(e) => onChange({ ...ex, name: e.target.value })} />
        </label>
        {!fn && (
          <label className="row">
            <input type="checkbox" checked={!!ex.ordered} onChange={(e) => onChange({ ...ex, ordered: e.target.checked })} />
            {t("custom.editor.ordered")}
          </label>
        )}
      </div>
      {(values.length > 0 || (!fn && columnParams.length > 0)) && (
        <div className="pv-custom-args">
          {!fn &&
            columnParams.map((p) => (
              <label key={p.name} className="row">
                <span className="pv-custom-param-name">{p.name}</span>
                <select value={ex.args?.[p.name] ?? ""} onChange={(e) => setArg(p.name, e.target.value || null)}>
                  <option value="">—</option>
                  {[...new Set([...inputColumns, ...Object.values(ex.tables ?? {}).flatMap((x) => x.columns.map((c) => c.name))])].map((c) => (
                    <option key={c} value={c}>
                      {c}
                    </option>
                  ))}
                </select>
              </label>
            ))}
          {values.map((p) => (
            <label key={p.name} className="row">
              <span className="pv-custom-param-name">{p.name}</span>
              <input
                type="text"
                className="pv-mono"
                placeholder={p.optional ? (p.default ?? "∅") : ""}
                value={ex.args && p.name in ex.args ? cellToInput(ex.args[p.name] ?? null) : ""}
                onChange={(e) => {
                  const args = { ...(ex.args ?? {}) };
                  if (e.target.value === "" && p.optional) delete args[p.name];
                  else args[p.name] = cellFromInput(e.target.value);
                  onChange({ ...ex, args });
                }}
              />
            </label>
          ))}
        </div>
      )}
      <span className="pv-section-title">{t(fn ? "custom.editor.input_and_expected" : "custom.editor.input")}</span>
      <Grid
        t={t}
        table={ex.input}
        types
        editColumns={!fn}
        onChange={(input) => onChange(fn ? syncFunctionExample(def, { ...ex, input }) : { ...ex, input })}
        extra={
          fn && !summary
            ? {
                header: t("custom.editor.expected"),
                cell: (r) => (
                  <span className={expectedBad(r) ? "pv-custom-bad" : undefined} title={expectedBad(r) ? t("custom.editor.actual", { value: outcome?.actual?.rows[r]?.[0] ?? "∅" }) : undefined}>
                    <input
                      type="text"
                      className="pv-mono"
                      placeholder="∅"
                      value={cellToInput(ex.expected.rows[r]?.[0] ?? null)}
                      onChange={(e) => setExpectedCell(r, e.target.value)}
                    />
                  </span>
                ),
              }
            : undefined
        }
      />
      <div className="row">
        <button onClick={() => onChange(fn ? syncFunctionExample(def, { ...ex, input: addRow(ex.input) }) : { ...ex, input: addRow(ex.input) })}>
          <Icon name="plus" /> {t("custom.editor.add_row")}
        </button>
        {!fn && (
          <button onClick={() => onChange({ ...ex, input: addColumn(ex.input) })}>
            <Icon name="plus" /> {t("custom.editor.add_column")}
          </button>
        )}
      </div>
      {otherTables.map((p) => {
        const table = ex.tables?.[p.name] ?? { columns: [{ name: "a", dtype: "VARCHAR" }], rows: [] };
        const setTable = (x: ExampleTable) => onChange({ ...ex, tables: { ...(ex.tables ?? {}), [p.name]: x } });
        return (
          <div key={p.name} className="col">
            <span className="pv-section-title">{t("custom.editor.table_input", { table: p.name })}</span>
            <Grid t={t} table={table} types editColumns onChange={setTable} />
            <div className="row">
              <button onClick={() => setTable(addRow(table))}>
                <Icon name="plus" /> {t("custom.editor.add_row")}
              </button>
              <button onClick={() => setTable(addColumn(table))}>
                <Icon name="plus" /> {t("custom.editor.add_column")}
              </button>
            </div>
          </div>
        );
      })}
      {summary && (
        <label className={`row ${marks.length ? "pv-custom-bad" : ""}`}>
          <span className="pv-section-title">{t("custom.editor.expected_value")}</span>
          <input
            type="text"
            className="grow pv-mono"
            placeholder="∅"
            value={cellToInput(ex.expected.rows[0]?.[0] ?? null)}
            onChange={(e) => setExpectedCell(0, e.target.value)}
          />
          {marks.length > 0 && <span className="pv-hint">{t("custom.editor.actual", { value: outcome?.actual?.rows[0]?.[0] ?? "∅" })}</span>}
        </label>
      )}
      {!fn && (
        <>
          <span className="pv-section-title">{t("custom.editor.expected")}</span>
          <Grid t={t} table={ex.expected} editColumns marks={marks} actual={outcome?.actual} onChange={(expected) => onChange({ ...ex, expected })} />
          <div className="row">
            <button onClick={() => onChange({ ...ex, expected: addRow(ex.expected) })}>
              <Icon name="plus" /> {t("custom.editor.add_row")}
            </button>
            <button onClick={() => onChange({ ...ex, expected: addColumn(ex.expected, false) })}>
              <Icon name="plus" /> {t("custom.editor.add_column")}
            </button>
          </div>
          {outcome?.actual && marks.some(([r, c]) => r === -1 && c === -1) && (
            <div className="pv-hint">
              {t("custom.editor.actual_columns", { columns: outcome.actual.columns.map((c) => c.name).join(", ") })}
            </div>
          )}
        </>
      )}
      {outcome?.error && <div className="error-text small">{outcome.error}</div>}
      <div className="row">
        <button disabled={props.busy} onClick={props.onFill} title={t("custom.editor.fill_hint")}>
          <Icon name="run" /> {t("custom.editor.fill")}
        </button>
      </div>
    </div>
  );
}

function ParamRow(props: {
  t: Ui["t"];
  def: CustomDefinition;
  p: CustomParam;
  typeSets: string[];
  valueTypes: string[];
  onChange(p: CustomParam): void;
  onRemove(): void;
}) {
  const { t, def, p, onChange } = props;
  const tables = def.params.filter((x) => x.kind === "table");
  const kinds = def.kind === "function" ? (["column", "value"] as const) : (["table", "column", "value"] as const);
  return (
    <tr>
      <td>
        <input type="text" className="pv-mono" value={p.name} aria-label={t("custom.editor.param_name")} onChange={(e) => onChange({ ...p, name: e.target.value })} />
      </td>
      <td>
        <select
          value={p.kind}
          onChange={(e) => {
            const kind = e.target.value as CustomParam["kind"];
            onChange({
              name: p.name,
              kind,
              ...(kind === "column" ? { type: ["any"] } : kind === "value" ? { dtype: "VARCHAR" } : {}),
              description: p.description,
            });
          }}
        >
          {kinds.map((k) => (
            <option key={k} value={k}>
              {t(`custom.kind.${k}`)}
            </option>
          ))}
        </select>
      </td>
      <td>
        {p.kind === "column" && (
          <select value={p.type?.[0] ?? "any"} onChange={(e) => onChange({ ...p, type: [e.target.value] })}>
            {props.typeSets.map((s) => (
              <option key={s} value={s}>
                {t(`custom.type.${s}`)}
              </option>
            ))}
          </select>
        )}
        {p.kind === "value" && (
          <select value={p.dtype ?? "VARCHAR"} onChange={(e) => onChange({ ...p, dtype: e.target.value })}>
            {props.valueTypes.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </select>
        )}
        {p.kind === "table" && (
          <input
            type="text"
            className="pv-mono"
            placeholder={t("custom.editor.required_columns")}
            title={t("custom.editor.required_columns_hint")}
            defaultValue={formatRequiredColumns(p.columns)}
            onBlur={(e) => onChange({ ...p, columns: parseRequiredColumns(e.target.value) })}
          />
        )}
      </td>
      <td>
        {p.kind === "value" && (
          <span className="row">
            <label className="row" title={t("custom.editor.optional_hint")}>
              <input type="checkbox" checked={!!p.optional} onChange={(e) => onChange({ ...p, optional: e.target.checked })} />
              {t("custom.editor.optional")}
            </label>
            {p.optional && (
              <input
                type="text"
                className="pv-mono pv-custom-default"
                placeholder={t("custom.editor.default")}
                value={p.default ?? ""}
                onChange={(e) => onChange({ ...p, default: e.target.value === "" ? undefined : e.target.value })}
              />
            )}
          </span>
        )}
        {p.kind === "column" && def.kind === "operation" && tables.length > 1 && (
          <select value={p.of ?? tables[0]?.name ?? ""} onChange={(e) => onChange({ ...p, of: e.target.value })} title={t("custom.editor.of")}>
            {tables.map((x) => (
              <option key={x.name} value={x.name}>
                {t("custom.editor.of_table", { table: x.name })}
              </option>
            ))}
          </select>
        )}
      </td>
      <td>
        <input type="text" value={p.description ?? ""} placeholder={t("custom.editor.param_description")} onChange={(e) => onChange({ ...p, description: e.target.value || undefined })} />
      </td>
      <td>
        <button className="icon" title={t("custom.editor.remove_param")} onClick={props.onRemove}>
          <Icon name="close" size={16} />
        </button>
      </td>
    </tr>
  );
}

/** Define or edit a custom function or operation: signature, SQL body,
 * examples; Validate runs every check, Save only after it passes. */
function CustomEditorDialog({ ui, close, a }: { ui: Ui; close: Closer<boolean>; a: Args }) {
  const t = ui.t;
  const [def, setDef] = useState<CustomDefinition>(() => {
    const d: CustomDefinition = JSON.parse(JSON.stringify(a.definition));
    d.examples = d.examples.map((ex) => syncFunctionExample(d, ex));
    return d;
  });
  const [report, setReport] = useState<CustomReport | null>(a.definition.report ?? null);
  const [validatedKey, setValidatedKey] = useState<string | null>(null);
  const [exampleIdx, setExampleIdx] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const fn = def.kind === "function";
  const currentKey = JSON.stringify(definitionToSave(def));
  const fresh = validatedKey === currentKey;
  const canSave = fresh && reportPassed(report) && !busy;

  /** Change the definition; function examples follow the signature. */
  const update = (patch: Partial<CustomDefinition>) =>
    setDef((d) => {
      const next = { ...d, ...patch };
      if (patch.params || patch.result) next.examples = next.examples.map((ex) => syncFunctionExample(next, ex));
      return next;
    });
  const setParam = (i: number, p: CustomParam) => update({ params: def.params.map((x, j) => (j === i ? p : x)) });
  const setExample = (i: number, ex: CustomExample) => update({ examples: def.examples.map((x, j) => (j === i ? ex : x)) });

  const validate = async (): Promise<CustomReport | null> => {
    setBusy(true);
    setError("");
    try {
      const r = await api.customValidate(definitionToSave(def), a.editing);
      setReport(r);
      setValidatedKey(currentKey);
      return r;
    } catch (e) {
      setError(errorMessage(e));
      return null;
    } finally {
      setBusy(false);
    }
  };

  const fill = async () => {
    const r = await validate();
    const actual = r?.examples[exampleIdx]?.actual;
    if (!actual) {
      if (r && !r.examples[exampleIdx]?.error) setError(t("custom.editor.fill_failed"));
      return;
    }
    const ex = def.examples[exampleIdx];
    setExample(exampleIdx, { ...ex, expected: expectedFromActual(def, actual) });
    setValidatedKey(null);
  };

  const save = async (draft: boolean) => {
    setBusy(true);
    setError("");
    try {
      const r = await api.customSave(definitionToSave(def), a.editing, draft);
      setReport(r.report);
      setValidatedKey(currentKey);
      if (r.saved) {
        ui.status(t(r.status === "ok" ? "custom.status.saved" : "custom.status.saved_draft", { name: def.name }), 4000);
        close(true);
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const schema = useMemo(() => {
    const tables =
      def.kind === "function"
        ? [{ name: "params", columns: def.params.map((p) => p.name) }]
        : def.params
            .filter((p) => p.kind === "table")
            .map((tp, k) => ({
              name: tp.name,
              columns: [
                ...(tp.columns ?? []).map((c) => c.name),
                ...def.params.filter((p) => p.kind === "column" && (p.of ? p.of === tp.name : k === 0)).map((p) => p.name),
              ],
            }));
    return { tables, functions: [], methods: [] };
  }, [def.kind, def.params]);

  const sameAsChoices = def.params.filter((p) => p.kind === "column").map((p) => p.name);
  const resultTypeValue = def.result.same_as ? `same:${def.result.same_as}` : (def.result.type?.[0] ?? "any");
  const ex = def.examples[exampleIdx];
  const outcome = fresh || report ? report?.examples[exampleIdx] : undefined;

  return (
    <Dialog
      title={t(a.editing ? "custom.editor.title_edit" : fn ? "custom.editor.title_new_function" : "custom.editor.title_new_operation", { name: a.editing ?? "" })}
      onClose={() => close(false)}
      className="pv-dialog-custom-editor"
      footer={
        <>
          <div className="left">
            <button disabled={busy} onClick={() => void validate()}>
              <Icon name="check" /> {t("custom.editor.validate")}
            </button>
            {busy && <span className="pv-hint">{t("custom.editor.validating")}</span>}
          </div>
          <button onClick={() => close(false)}>{t("btn.cancel")}</button>
          <button disabled={busy || !def.name.trim()} onClick={() => void save(true)} title={t("custom.editor.save_draft_hint")}>
            {t("custom.editor.save_draft")}
          </button>
          <button className="primary" disabled={!canSave} onClick={() => void save(false)} title={canSave ? undefined : t("custom.editor.save_hint")}>
            {t("btn.save")}
          </button>
        </>
      }
    >
      <div className="pv-custom-editor">
        <div className="pv-custom-head">
          <label className="row">
            <span>{t("custom.editor.name")}</span>
            <input
              type="text"
              className="grow pv-mono"
              autoFocus
              placeholder="my_function"
              value={def.name}
              onChange={(e) => update({ name: e.target.value })}
            />
          </label>
          <label className="row">
            <span>{t("custom.editor.label")}</span>
            <input type="text" className="grow" value={def.title ?? ""} onChange={(e) => update({ title: e.target.value })} />
          </label>
          <label className="row pv-custom-wide">
            <span>{t("custom.editor.description")}</span>
            <input type="text" className="grow" value={def.description ?? ""} onChange={(e) => update({ description: e.target.value })} />
          </label>
        </div>

        <fieldset>
          <legend>{t("custom.editor.signature")}</legend>
          <table className="simple pv-custom-params-table">
            <thead>
              <tr>
                <th>{t("custom.editor.param_name")}</th>
                <th>{t("custom.editor.param_kind")}</th>
                <th>{t("custom.editor.param_type")}</th>
                <th />
                <th>{t("custom.editor.param_description")}</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {def.params.map((p, i) => (
                <ParamRow
                  key={i}
                  t={t}
                  def={def}
                  p={p}
                  typeSets={a.typeSets}
                  valueTypes={a.valueTypes}
                  onChange={(np) => setParam(i, np)}
                  onRemove={() => update({ params: def.params.filter((_, j) => j !== i) })}
                />
              ))}
            </tbody>
          </table>
          <div className="row">
            <button
              onClick={() => {
                const taken = new Set(def.params.map((p) => p.name));
                let name = "p1";
                for (let i = 2; taken.has(name); i++) name = `p${i}`;
                update({ params: [...def.params, { name, kind: "column", type: ["any"] }] });
              }}
            >
              <Icon name="plus" /> {t("custom.editor.add_param")}
            </button>
            <span className="grow" />
            {fn ? (
              <>
                <span>{t("custom.editor.result")}</span>
                <select
                  value={def.result.kind}
                  onChange={(e) => update({ result: { ...def.result, kind: e.target.value as "column" | "value" } })}
                >
                  <option value="column">{t("custom.result.column")}</option>
                  <option value="value">{t("custom.result.value")}</option>
                </select>
                <select
                  value={resultTypeValue}
                  onChange={(e) => {
                    const v = e.target.value;
                    update({
                      result: v.startsWith("same:")
                        ? { kind: def.result.kind, same_as: v.slice(5) }
                        : { kind: def.result.kind, type: [v] },
                    });
                  }}
                >
                  {a.typeSets.map((s) => (
                    <option key={s} value={s}>
                      {t(`custom.type.${s}`)}
                    </option>
                  ))}
                  {sameAsChoices.map((n) => (
                    <option key={n} value={`same:${n}`}>
                      {t("custom.type.same_as", { param: n })}
                    </option>
                  ))}
                </select>
              </>
            ) : (
              <>
                <span>{t("custom.editor.placement")}</span>
                <select value={def.placement ?? "step"} onChange={(e) => update({ placement: e.target.value as "step" | "new_table" })}>
                  <option value="step">{t("custom.placement.step")}</option>
                  <option value="new_table">{t("custom.placement.new_table")}</option>
                </select>
              </>
            )}
          </div>
          <div className="pv-mono pv-hint pv-selectable">{def.name ? signatureText(def) : ""}</div>
        </fieldset>

        <div className="col">
          <div className="row">
            <span className="pv-section-title grow">{t(fn ? "custom.editor.body_function" : "custom.editor.body_operation")}</span>
            <label className="row" title={t("custom.editor.uses_today_hint")}>
              <input
                type="checkbox"
                checked={def.flags.uses_current_time}
                onChange={(e) => update({ flags: { uses_current_time: e.target.checked } })}
              />
              {t("custom.editor.uses_today")}
            </label>
          </div>
          <div className="pv-custom-body">
            <QueryEditor
              mode="sql"
              value={def.body.source}
              onChange={(v) => update({ body: { language: "sql", source: v } })}
              onRun={() => void validate()}
              schema={schema}
              keywords={[]}
              fontFamily={String(ui.settings.default_sql_font || "monospace")}
              fontSize={10}
            />
          </div>
          <div className="pv-hint">{t(fn ? "custom.editor.body_function_hint" : "custom.editor.body_operation_hint")}</div>
          {report?.compiled && (
            <details>
              <summary>{t("custom.editor.compiled")}</summary>
              <pre className="pv-mono pv-selectable pv-custom-compiled">{report.compiled.template}</pre>
              <div className="pv-hint">
                {Object.entries(report.compiled.result_types)
                  .map(([k, v]) => `${k} → ${v}`)
                  .join("; ")}
              </div>
            </details>
          )}
        </div>

        <fieldset>
          <legend>{t("custom.editor.examples")}</legend>
          <div className="tabs-strip" role="tablist">
            {def.examples.map((x, i) => {
              const o = report?.examples[i];
              return (
                <button key={i} role="tab" aria-selected={i === exampleIdx} className={i === exampleIdx ? "active" : ""} onClick={() => setExampleIdx(i)}>
                  {o && fresh && <Icon name={o.passed ? "checkCircle" : "failCircle"} size={14} className={o.passed ? "pv-ok" : "pv-bad"} />}
                  {x.name || t("custom.editor.example_n", { n: i + 1 })}
                </button>
              );
            })}
            <button
              title={t("custom.editor.add_example")}
              onClick={() => {
                update({ examples: [...def.examples, newExample(def, def.examples.length + 1)] });
                setExampleIdx(def.examples.length);
              }}
            >
              <Icon name="plus" size={16} />
            </button>
          </div>
          {ex ? (
            <>
              <ExampleEditor t={t} def={def} ex={ex} outcome={outcome} onChange={(x) => setExample(exampleIdx, x)} onFill={() => void fill()} busy={busy} />
              <div className="row">
                <button
                  onClick={() => {
                    const copy: CustomExample = JSON.parse(JSON.stringify(ex));
                    copy.name = `${ex.name} 2`;
                    update({ examples: [...def.examples, copy] });
                    setExampleIdx(def.examples.length);
                  }}
                >
                  <Icon name="duplicate" /> {t("custom.editor.duplicate_example")}
                </button>
                <button
                  className="danger"
                  onClick={() => {
                    update({ examples: def.examples.filter((_, j) => j !== exampleIdx) });
                    setExampleIdx(Math.max(0, exampleIdx - 1));
                  }}
                >
                  {t("custom.editor.remove_example")}
                </button>
                <span className="pv-hint grow">{t("custom.editor.cells_hint")}</span>
              </div>
            </>
          ) : (
            <div className="pv-hint">{t("custom.editor.no_examples")}</div>
          )}
        </fieldset>

        {error && <div className="error-text">{error}</div>}
        {report && (
          <fieldset className={fresh ? "" : "pv-custom-stale"}>
            <legend>{t(fresh ? "custom.editor.report" : "custom.editor.report_stale")}</legend>
            <ul className="pv-custom-report">
              {report.checks.map((c, i) => (
                <li key={i} className={`pv-check-${c.status}`}>
                  <button
                    className="text"
                    disabled={c.example === undefined}
                    onClick={() => c.example !== undefined && setExampleIdx(c.example)}
                  >
                    <Icon name={STATUS_ICON[c.status]} size={16} />
                    <span className="pv-selectable">{customCheckText(t, c.message)}</span>
                  </button>
                </li>
              ))}
            </ul>
          </fieldset>
        )}
        {!report && <div className="pv-hint">{t("custom.editor.validate_first")}</div>}
      </div>
    </Dialog>
  );
}

export function openCustomEditor(ui: Ui, a: Args): Promise<boolean> {
  return ui.showDialog<boolean>((close) => <CustomEditorDialog ui={ui} close={close} a={a} />, false);
}
