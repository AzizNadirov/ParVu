// Condition rows shared by Filter Rows and the "Only rows where…" part of
// column operations: column, an operator that fits its type, and a value
// box that fits it too (number, date, date and time, time, yes/no, or text
// with the column's frequent values as suggestions).
import { useEffect, useId, useRef, useState } from "react";
import type { ColumnMeta, Combine, CondOp, Condition, RowFilter } from "../lib/api";
import type { Translate } from "../lib/i18n";
import { Icon } from "../components/Icon";
import { ColumnSelect } from "./common";
import { buildRowFilter, condArity, condIsTextual, emptyCondition, isTextType, opsFor, valueKind, type ValueKind } from "./logic";

/** Values to suggest for a column (its most frequent values). */
export type Suggest = (column: string) => Promise<string[]>;

const INPUT_TYPE: Record<ValueKind, string> = {
  number: "number",
  date: "date",
  datetime: "datetime-local",
  time: "time",
  boolean: "text",
  text: "text",
};

export function kindOf(columns: ColumnMeta[], name: string): ValueKind {
  const meta = columns.find((c) => c.name === name);
  return valueKind(meta?.family, meta?.dtype ?? "");
}

/** One value box, typed by the column. Lists (in, not in) stay text. */
function ValueInput(props: {
  t: Translate;
  kind: ValueKind;
  list: boolean;
  value: string;
  onChange(v: string): void;
  suggestions: string[];
  placeholder?: string;
}) {
  const { t, kind, list } = props;
  const listId = useId();
  if (kind === "boolean" && !list) {
    return (
      <select className="grow" value={props.value} onChange={(e) => props.onChange(e.target.value)}>
        <option value="">—</option>
        <option value="true">{t("cond.true")}</option>
        <option value="false">{t("cond.false")}</option>
      </select>
    );
  }
  const type = list ? "text" : INPUT_TYPE[kind];
  return (
    <>
      <input
        type={type}
        step={type === "number" ? "any" : type === "datetime-local" || type === "time" ? 1 : undefined}
        className="grow"
        placeholder={props.placeholder}
        value={props.value}
        list={kind === "text" && !list && props.suggestions.length ? listId : undefined}
        onChange={(e) => props.onChange(e.target.value)}
      />
      {kind === "text" && !list && props.suggestions.length > 0 && (
        <datalist id={listId}>
          {props.suggestions.map((v) => (
            <option key={v} value={v} />
          ))}
        </datalist>
      )}
    </>
  );
}

export function ConditionRows(props: {
  t: Translate;
  columns: ColumnMeta[];
  conditions: Condition[];
  onChange(conditions: Condition[]): void;
  suggest?: Suggest;
}) {
  const { t, columns, conditions, onChange } = props;
  const names = columns.map((c) => c.name);
  const [suggestions, setSuggestions] = useState<Record<string, string[]>>({});

  // Suggestions for the text columns in use, fetched once each.
  const requested = useRef(new Set<string>());
  const textColumns = [...new Set(conditions.map((c) => c.column))].filter((c) => kindOf(columns, c) === "text");
  const key = textColumns.join("\u0000");
  useEffect(() => {
    const suggest = props.suggest;
    if (!suggest) return;
    for (const c of textColumns) {
      if (requested.current.has(c)) continue;
      requested.current.add(c);
      suggest(c)
        .then((v) => setSuggestions((s) => ({ ...s, [c]: v })))
        .catch(() => undefined);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  const update = (i: number, patch: Partial<Condition>) => {
    onChange(
      conditions.map((x, j) => {
        if (j !== i) return x;
        const next = { ...x, ...patch };
        // Another column: keep the operator if it still fits, and drop
        // a value typed for the old column's type.
        if (patch.column !== undefined && patch.column !== x.column) {
          const ops = opsFor(kindOf(columns, next.column));
          if (!ops.includes(next.op)) next.op = ops[0];
          if (kindOf(columns, next.column) !== kindOf(columns, x.column)) {
            next.value = "";
            next.value2 = "";
          }
        }
        return next;
      }),
    );
  };

  return (
    <>
      <div className="pv-rules">
        {conditions.map((c, i) => {
          const kind = kindOf(columns, c.column);
          const ops = opsFor(kind);
          const arity = condArity(c.op);
          const list = c.op === "in" || c.op === "not_in";
          const isText = kind === "text";
          return (
            <div key={i} className="row">
              <ColumnSelect columns={names} value={c.column} onChange={(v) => update(i, { column: v })} className="" />
              <select value={c.op} onChange={(e) => update(i, { op: e.target.value as CondOp })}>
                {ops.map((o) => (
                  <option key={o} value={o}>
                    {t(`cond.${o}`)}
                  </option>
                ))}
              </select>
              {arity >= 1 && (
                <ValueInput
                  t={t}
                  kind={kind}
                  list={list}
                  value={c.value}
                  onChange={(v) => update(i, { value: v })}
                  suggestions={suggestions[c.column] ?? []}
                  placeholder={list ? t("dialog.filter.list_placeholder") : t("dialog.filter.value")}
                />
              )}
              {arity === 2 && (
                <>
                  {t("dialog.filter.and")}
                  <ValueInput t={t} kind={kind} list={false} value={c.value2} onChange={(v) => update(i, { value2: v })} suggestions={[]} />
                </>
              )}
              {arity === 0 && <span className="grow" />}
              {isText && (
                <label className="row" title={t("dialog.filter.case_sensitive")}>
                  <input
                    type="checkbox"
                    disabled={!condIsTextual(c.op)}
                    checked={c.case_sensitive}
                    onChange={(e) => update(i, { case_sensitive: e.target.checked })}
                  />
                  Aa
                </label>
              )}
              <button
                className="pv-icon-btn"
                title={t("btn.remove")}
                aria-label={t("btn.remove")}
                disabled={conditions.length === 1}
                onClick={() => onChange(conditions.filter((_, j) => j !== i))}
              >
                <Icon name="close" />
              </button>
            </div>
          );
        })}
      </div>
      <div className="row">
        <button onClick={() => onChange([...conditions, emptyCondition(conditions[conditions.length - 1]?.column ?? names[0] ?? "")])}>
          {t("dialog.filter.add")}
        </button>
      </div>
    </>
  );
}

/** "All / any of these" for a list of conditions. */
export function CombineSelect(props: { t: Translate; value: Combine; onChange(c: Combine): void }) {
  return (
    <select value={props.value} onChange={(e) => props.onChange(e.target.value as Combine)}>
      <option value="and">{props.t("dialog.filter.all")}</option>
      <option value="or">{props.t("dialog.filter.any")}</option>
    </select>
  );
}

/** State of an "Only rows where…" section. */
export interface WhereState {
  on: boolean;
  conditions: Condition[];
  combine: Combine;
}

export const initialWhere = (column: string, on = false): WhereState => ({ on, conditions: [emptyCondition(column)], combine: "and" });

/** The section's row filter: undefined when off, null while incomplete. */
export function whereFilter(w: WhereState, columns: ColumnMeta[]): RowFilter | null | undefined {
  if (!w.on) return undefined;
  return buildRowFilter(w.conditions, w.combine, (name) => isTextType(columns.find((c) => c.name === name)?.dtype ?? ""));
}

/** "Only rows where…": a check box that opens condition rows. `required`
 * keeps it open (Set Value always needs rows). */
export function WhereSection(props: {
  t: Translate;
  columns: ColumnMeta[];
  value: WhereState;
  onChange(w: WhereState): void;
  suggest?: Suggest;
  required?: boolean;
}) {
  const { t, value: w } = props;
  const set = (patch: Partial<WhereState>) => props.onChange({ ...w, ...patch });
  return (
    <fieldset className="pv-where">
      <legend>
        {props.required ? (
          t("where.title")
        ) : (
          <label className="row">
            <input type="checkbox" checked={w.on} onChange={(e) => set({ on: e.target.checked })} />
            {t("where.only")}
          </label>
        )}
      </legend>
      {(w.on || props.required) && (
        <>
          <label className="row">
            {t("where.match")}
            <CombineSelect t={t} value={w.combine} onChange={(combine) => set({ combine })} />
          </label>
          <ConditionRows
            t={t}
            columns={props.columns}
            conditions={w.conditions}
            onChange={(conditions) => set({ conditions })}
            suggest={props.suggest}
          />
          <div className="pv-hint">{t(props.required ? "where.hint_set" : "where.hint")}</div>
        </>
      )}
    </fieldset>
  );
}
