// Column functions of the type-aware column menu: which take parameters,
// the fields of their dialog, and building the function from those fields.
import type { Cell, ColumnFn, FnOutput, Side } from "./api";

export type FnName = ColumnFn["fn"];

/** One-click text functions, applied in place (Text ▸). */
export const TEXT_FNS: ColumnFn[] = [
  { fn: "trim", side: "both" },
  { fn: "trim", side: "start" },
  { fn: "trim", side: "end" },
  { fn: "clean" },
  { fn: "collapse_spaces" },
  { fn: "lower" },
  { fn: "upper" },
  { fn: "proper" },
  { fn: "remove_accents" },
];

/** Menu label key of a function (trim gets one per side). */
export function fnLabelKey(f: ColumnFn): string {
  return f.fn === "trim" && f.side !== "both" ? `fn.trim_${f.side}` : `fn.${f.fn}`;
}

export type FieldKind = "text" | "number" | "bool" | "side";

export interface Field {
  key: string;
  kind: FieldKind;
  initial: string | number | boolean;
  /** Smallest allowed number. */
  min?: number;
}

/** Functions with a dialog: their fields, and whether they write to a new column by default. */
export const FN_DIALOGS: Partial<Record<FnName, { fields: Field[]; output: FnOutput["to"] }>> = {
  add_prefix: { fields: [{ key: "text", kind: "text", initial: "" }], output: "in_place" },
  add_suffix: { fields: [{ key: "text", kind: "text", initial: "" }], output: "in_place" },
  pad: {
    fields: [
      { key: "length", kind: "number", initial: 8, min: 1 },
      { key: "fill", kind: "text", initial: "0" },
      { key: "side", kind: "side", initial: "start" },
    ],
    output: "in_place",
  },
  length: { fields: [], output: "new" },
  first: { fields: [{ key: "n", kind: "number", initial: 3, min: 1 }], output: "new" },
  last: { fields: [{ key: "n", kind: "number", initial: 3, min: 1 }], output: "new" },
  range: {
    fields: [
      { key: "start", kind: "number", initial: 1, min: 1 },
      { key: "length", kind: "number", initial: 3, min: 1 },
    ],
    output: "new",
  },
  before: {
    fields: [
      { key: "delimiter", kind: "text", initial: "-" },
      { key: "last", kind: "bool", initial: false },
    ],
    output: "new",
  },
  after: {
    fields: [
      { key: "delimiter", kind: "text", initial: "-" },
      { key: "last", kind: "bool", initial: false },
    ],
    output: "new",
  },
  between: {
    fields: [
      { key: "start", kind: "text", initial: "(" },
      { key: "end", kind: "text", initial: ")" },
    ],
    output: "new",
  },
  regex_extract: {
    fields: [
      { key: "pattern", kind: "text", initial: "" },
      { key: "group", kind: "number", initial: 0, min: 0 },
    ],
    output: "new",
  },
};

/** Extract ▸ entries, in menu order. */
export const EXTRACT_FNS: FnName[] = ["length", "first", "last", "range", "before", "after", "between", "regex_extract"];

export type FieldValues = Record<string, string | number | boolean>;

export function initialValues(name: FnName): FieldValues {
  const v: FieldValues = {};
  for (const f of FN_DIALOGS[name]?.fields ?? []) v[f.key] = f.initial;
  return v;
}

/** The function from a dialog's values, or null while one is missing or out of range. */
export function buildFn(name: FnName, v: FieldValues): ColumnFn | null {
  for (const f of FN_DIALOGS[name]?.fields ?? []) {
    const x = v[f.key];
    if (f.kind === "number" && (!Number.isInteger(Number(x)) || Number(x) < (f.min ?? 0))) return null;
    if (f.kind === "text" && typeof x === "string" && !x && f.key !== "text") return null;
  }
  const num = (k: string) => Number(v[k]);
  const str = (k: string) => String(v[k] ?? "");
  switch (name) {
    case "add_prefix":
    case "add_suffix":
      return str("text") ? { fn: name, text: str("text") } : null;
    case "pad":
      return { fn: "pad", length: num("length"), fill: str("fill"), side: v.side as Side };
    case "length":
      return { fn: "length" };
    case "first":
    case "last":
      return { fn: name, n: num("n") };
    case "range":
      return { fn: "range", start: num("start"), length: num("length") };
    case "before":
    case "after":
      return { fn: name, delimiter: str("delimiter"), last: !!v.last };
    case "between":
      return { fn: "between", start: str("start"), end: str("end") };
    case "regex_extract":
      return { fn: "regex_extract", pattern: str("pattern"), group: num("group") };
    default:
      return null;
  }
}

/** Up to `n` distinct non-empty values of a page column, for previews. */
export function previewValues(rows: Cell[][], col: number, n = 6): Cell[] {
  const seen = new Set<string>();
  for (const r of rows) {
    const v = r[col];
    if (v !== null && v !== "" && !seen.has(v)) seen.add(v);
    if (seen.size >= n) break;
  }
  return [...seen];
}
