// Column functions of the type-aware column menu: which take parameters,
// the fields of their dialog, and building the function from those fields.
import type { Cell, ColumnFn, FnOutput, MatchHow, RelativeRange, Side, TimeUnitName, TypeFamily } from "./api";

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

export type FieldKind = "text" | "number" | "bool" | "side" | "unit" | "format" | "how";

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
  add_time: {
    fields: [
      { key: "amount", kind: "number", initial: 1, min: -1_000_000 },
      { key: "unit", kind: "unit", initial: "day" },
    ],
    output: "in_place",
  },
  format_date: { fields: [{ key: "format", kind: "format", initial: "%d.%m.%Y" }], output: "new" },
  json_path: {
    fields: [
      { key: "path", kind: "text", initial: "$." },
      { key: "as_text", kind: "bool", initial: true },
    ],
    output: "new",
  },
  join_list: { fields: [{ key: "separator", kind: "text", initial: ", " }], output: "new" },
  matches: {
    fields: [
      { key: "how", kind: "how", initial: "contains" },
      { key: "find", kind: "text", initial: "" },
      { key: "ignore_case", kind: "bool", initial: true },
    ],
    output: "new",
  },
};

export const MATCH_HOWS: MatchHow[] = ["contains", "starts_with", "ends_with", "regex"];

/** Units offered when adding time. */
export const ADD_UNITS: TimeUnitName[] = ["day", "week", "month", "quarter", "year", "hour", "minute"];

/** strftime presets for "Format as text". */
export const DATE_FORMATS = ["%d.%m.%Y", "%d/%m/%Y", "%m/%d/%Y", "%Y-%m-%d", "%d %B %Y", "%B %Y", "%Y-%m", "%d.%m.%Y %H:%M", "%H:%M"];

export const isDateFamily = (f?: TypeFamily) => f === "date" || f === "timestamp" || f === "timestamp_tz";
export const hasTime = (f?: TypeFamily) => f === "timestamp" || f === "timestamp_tz" || f === "time";

/** Quick relative date filters (Filter dates ▸). */
export const DATE_FILTERS: { key: string; range: RelativeRange }[] = [
  { key: "dates.last_7_days", range: { kind: "last", n: 7, unit: "day" } },
  { key: "dates.last_30_days", range: { kind: "last", n: 30, unit: "day" } },
  { key: "dates.this_week", range: { kind: "this", unit: "week" } },
  { key: "dates.previous_week", range: { kind: "previous", unit: "week" } },
  { key: "dates.this_month", range: { kind: "this", unit: "month" } },
  { key: "dates.previous_month", range: { kind: "previous", unit: "month" } },
  { key: "dates.this_quarter", range: { kind: "this", unit: "quarter" } },
  { key: "dates.this_year", range: { kind: "this", unit: "year" } },
  { key: "dates.previous_year", range: { kind: "previous", unit: "year" } },
  { key: "dates.year_to_date", range: { kind: "year_to_date" } },
];

/** Extract ▸ entries, in menu order. */
export const EXTRACT_FNS: FnName[] = ["length", "first", "last", "range", "before", "after", "between", "regex_extract", "matches"];

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
    if (f.kind === "text" && typeof x === "string" && !x && f.key !== "text" && f.key !== "separator") return null;
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
    case "add_time":
      return num("amount") === 0 ? null : { fn: "add_time", n: num("amount"), unit: v.unit as TimeUnitName };
    case "format_date":
      return { fn: "format_date", format: str("format") };
    case "json_path":
      // "$" is the whole value; "$." is a path still being typed.
      return str("path").startsWith("$") && !str("path").endsWith(".") ? { fn: "json_path", path: str("path"), text: !!v.as_text } : null;
    case "join_list":
      return { fn: "join_list", separator: str("separator") };
    case "matches":
      return { fn: "matches", text: str("find"), how: v.how as MatchHow, ignore_case: !!v.ignore_case };
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
