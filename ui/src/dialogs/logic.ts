// Pure helpers behind the dialogs, kept free of React and Tauri so they can
// be unit-tested.
import type {
  AggFunc,
  Aggregation,
  BinMethod,
  Cell,
  ColumnMeta,
  Combine,
  CondOp,
  Condition,
  FillMethod,
  RowFilter,
  TypeFamily,
  SliceMode,
  SortKey,
  WindowFunc,
  WindowSpec,
  ColumnSpec,
  ColumnStats,
  CsvOptions,
  ExportFormat,
  ExportOptions,
  ExcelEntry,
  FunctionDef,
  JoinType,
  Keep,
  Operation,
  Reading,
  Quoting,
  ReplaceRequest,
  SearchHit,
  SearchOptions,
  CustomArg,
  CustomDefinition,
  CustomEntry,
  CustomExample,
  CustomParam,
  ExampleTable,
  FnOutput,
} from "../lib/api";
import type { Translate } from "../lib/i18n";

// ---------------------------------------------------------------- operations

export const JOIN_TYPES: { value: JoinType; label: string }[] = [
  { value: "inner", label: "INNER" },
  { value: "left", label: "LEFT" },
  { value: "right", label: "RIGHT" },
  { value: "full_outer", label: "FULL OUTER" },
];

export function buildJoinOp(right: string, joinType: JoinType, leftKey: string, rightKey: string): Operation | null {
  if (!right || !leftKey || !rightKey) return null;
  return { op: "join", right, join_type: joinType, left_key: leftKey, right_key: rightKey };
}

export function buildAppendOp(tables: string[], distinct: boolean): Operation | null {
  if (tables.length === 0) return null;
  return { op: "append", tables: [...tables], distinct };
}

/** Keeps the dialog's column order; no selection means "all columns". */
export function buildDropDuplicatesOp(allColumns: string[], selected: Iterable<string>, keep: Keep): Operation {
  const set = new Set(selected);
  return { op: "drop_duplicates", columns: allColumns.filter((c) => set.has(c)), keep };
}

/** A yes/no column for duplicated rows; null without a name. */
export function buildMarkDuplicatesOp(allColumns: string[], selected: Iterable<string>, keep: Keep, name: string): Operation | null {
  const set = new Set(selected);
  if (!name.trim()) return null;
  return { op: "mark_duplicates", columns: allColumns.filter((c) => set.has(c)), keep, name: name.trim() };
}

/** `null_value` is null for real NULLs, otherwise the sentinel text as typed. */
export function buildDropNullOp(column: string, useSentinel: boolean, sentinel: string): Operation | null {
  if (!column) return null;
  return { op: "drop_null", column, null_value: useSentinel ? sentinel : null };
}

export function defaultReplaceOutput(column: string): string {
  return `${column}_replaced`;
}

export function buildReplaceRequest(r: {
  column: string;
  pattern: string;
  replacement: string;
  output: string;
  caseSensitive: boolean;
  regex: boolean;
}): ReplaceRequest | null {
  const output = r.output.trim();
  if (!r.column || !r.pattern || !output) return null;
  return {
    column: r.column,
    pattern: r.pattern,
    replacement: r.replacement,
    output_column: output,
    case_sensitive: r.caseSensitive,
    regex: r.regex,
  };
}

// ---------------------------------------------------------------- unique values

/** Values sorted case-insensitively, NULL first (Python sorted by str(x).lower()). */
const asNumber = (v: string): number | null => {
  const t = v.trim();
  if (!t) return null;
  const n = Number(t);
  return Number.isFinite(n) ? n : null;
};

/** Order of unique values: NULL first, numbers by value, then text (case-insensitive). */
export function compareUniqueValues(a: Cell, b: Cell): number {
  if (a === null || b === null) return a === b ? 0 : a === null ? -1 : 1;
  const na = asNumber(a);
  const nb = asNumber(b);
  if (na !== null && nb !== null) return na - nb || (a < b ? -1 : a > b ? 1 : 0);
  if (na !== null) return -1;
  if (nb !== null) return 1;
  const la = a.toLowerCase();
  const lb = b.toLowerCase();
  return la < lb ? -1 : la > lb ? 1 : a < b ? -1 : a > b ? 1 : 0;
}

export function sortUniqueValues(values: Cell[]): Cell[] {
  return [...values].sort(compareUniqueValues);
}

/** Add newly found values to a sorted list, keeping it sorted and without repeats. */
export function mergeUniqueValues(sorted: Cell[], fresh: Cell[]): Cell[] {
  const have = new Set(sorted);
  const add = fresh.filter((v) => !have.has(v) && (have.add(v), true));
  if (!add.length) return sorted;
  return [...sorted, ...add].sort(compareUniqueValues);
}

/** Case-insensitive substring filter. NULL matches the text "null". */
export function filterUniqueValues(values: Cell[], query: string): Cell[] {
  const q = query.trim().toLowerCase();
  if (!q) return values;
  return values.filter((v) => (v === null ? "null" : v.toLowerCase()).includes(q));
}

// ---------------------------------------------------------------- column stats

export function fmtNum(x: number | null | undefined): string {
  if (x === null || x === undefined || Number.isNaN(x)) return "";
  if (Math.abs(x - Math.round(x)) < 1e-9 && Math.abs(x) < 1e15) {
    return Math.round(x).toLocaleString("en-US");
  }
  // Python's {:,.6g}
  const p = Number(x.toPrecision(6));
  const exp = Math.floor(Math.log10(Math.abs(p)));
  if (exp < -4 || exp >= 6) return p.toExponential(5).replace(/\.?0+e/, "e").replace(/e([+-])(\d)$/, "e$10$2");
  const decimals = Math.max(0, 5 - exp);
  return p.toLocaleString("en-US", { maximumFractionDigits: decimals, minimumFractionDigits: 0 });
}

export function fmtPct(numerator: number, denominator: number): string {
  if (denominator === 0) return "0%";
  const pct = (100 * numerator) / denominator;
  if (pct === 0) return "0%";
  if (pct < 0.01) return "<0.01%";
  return `${pct.toFixed(2)}%`;
}

/** [label key, value] rows shown by the column statistics dialog. */
export function statsRows(s: ColumnStats): [string, string][] {
  const rows: [string, string][] = [
    ["column_stats.rows", fmtNum(s.row_count)],
    ["column_stats.non_null", fmtNum(s.non_null)],
    ["column_stats.null", `${fmtNum(s.null)} (${fmtPct(s.null, s.row_count)})`],
    ["column_stats.distinct", (s.distinct_approx ? "≈ " : "") + fmtNum(s.distinct)],
    ["column_stats.min", s.min ?? "—"],
    ["column_stats.max", s.max ?? "—"],
  ];
  if (s.mean !== null) rows.push(["column_stats.mean", fmtNum(s.mean)]);
  if (s.std !== null) rows.push(["column_stats.std", fmtNum(s.std)]);
  return rows;
}

// ---------------------------------------------------------------- export

export type ExportFileType = "csv" | "tsv" | "parquet" | "json" | "jsonl" | "xlsx";

export const EXPORT_FILE_TYPES: ExportFileType[] = ["csv", "tsv", "parquet", "json", "jsonl", "xlsx"];

const EXT_TO_TYPE: Record<string, ExportFileType> = {
  csv: "csv",
  tsv: "tsv",
  parquet: "parquet",
  pq: "parquet",
  json: "json",
  jsonl: "jsonl",
  ndjson: "jsonl",
  xlsx: "xlsx",
};

export function fileExtension(path: string): string {
  const base = path.split(/[\\/]/).pop() ?? "";
  const dot = base.lastIndexOf(".");
  return dot > 0 ? base.slice(dot + 1).toLowerCase() : "";
}

/** File type for a path, from its extension; null when unknown or missing. */
export function fileTypeForPath(path: string): ExportFileType | null {
  return EXT_TO_TYPE[fileExtension(path)] ?? null;
}

export function exportFormatOf(type: ExportFileType): ExportFormat {
  switch (type) {
    case "csv":
    case "tsv":
      return "csv";
    case "parquet":
      return "parquet";
    case "json":
    case "jsonl":
      return "json";
    case "xlsx":
      return "excel";
  }
}

/** Replace (or add) the extension of `path` so it matches `type`. */
export function withExtension(path: string, type: ExportFileType): string {
  const trimmed = path.trim() || "export";
  const sep = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  const dir = trimmed.slice(0, sep + 1);
  const base = trimmed.slice(sep + 1) || "export";
  const dot = base.lastIndexOf(".");
  const stem = dot > 0 ? base.slice(0, dot) : base;
  return `${dir}${stem}.${type}`;
}

export function joinPath(dir: string | undefined, name: string): string {
  if (!dir) return name;
  const sep = dir.includes("\\") && !dir.includes("/") ? "\\" : "/";
  return dir.endsWith("/") || dir.endsWith("\\") ? dir + name : dir + sep + name;
}

/** True for DuckDB DATE, TIME and TIMESTAMP types. */
export function isTemporal(dtype: string): boolean {
  const t = dtype.toUpperCase();
  return t.startsWith("DATE") || t.startsWith("TIMESTAMP") || t.startsWith("TIME");
}

export const DATE_PRESETS = ["", "%Y-%m-%d", "%d/%m/%Y", "%m/%d/%Y", "%d.%m.%Y", "%Y%m%d"];
export const TIMESTAMP_PRESETS = ["", "%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%d/%m/%Y %H:%M", "%Y-%m-%d"];
export const DELIMITERS: { value: string; key: string; symbol: string }[] = [
  { value: ",", key: "dialog.export.delim.comma", symbol: "," },
  { value: ";", key: "dialog.export.delim.semicolon", symbol: ";" },
  { value: "\t", key: "dialog.export.delim.tab", symbol: "\\t" },
  { value: "|", key: "dialog.export.delim.pipe", symbol: "|" },
];

export interface ExportColumnState {
  name: string;
  dtype: string;
  include: boolean;
  cast: string;
}

export interface ExportFormState {
  path: string;
  type: ExportFileType;
  columns: ExportColumnState[];
  dateFormat: string;
  timestampFormat: string;
  delimiter: string;
  includeHeader: boolean;
  quoting: Quoting;
  compression: string;
}

/** Date formatting only shapes text output, and only matters with temporal columns. */
export function datesApplicable(type: ExportFileType, columns: { dtype: string }[]): boolean {
  return type !== "parquet" && columns.some((c) => isTemporal(c.dtype));
}

export function exportValidation(s: ExportFormState): "no_path" | "no_columns" | null {
  if (!s.path.trim()) return "no_path";
  if (!s.columns.some((c) => c.include)) return "no_columns";
  return null;
}

export function buildExportOptions(s: ExportFormState): ExportOptions {
  const format = exportFormatOf(s.type);
  const dates = datesApplicable(s.type, s.columns);
  const columns: ColumnSpec[] = s.columns.map((c) => ({
    name: c.name,
    source_type: c.dtype,
    include: c.include,
    cast_to: c.cast,
  }));
  return {
    path: s.path.trim(),
    format,
    columns,
    date_format: dates ? s.dateFormat.trim() : "",
    timestamp_format: dates ? s.timestampFormat.trim() : "",
    delimiter: s.type === "tsv" ? "\t" : s.delimiter,
    include_header: s.includeHeader,
    quoting: s.quoting,
    compression: s.compression,
    json_lines: s.type === "jsonl",
  };
}

export function initialExportState(
  tableName: string,
  columns: ColumnMeta[],
  defaultDir: string | undefined,
  defaultCompression: string,
): ExportFormState {
  return {
    path: joinPath(defaultDir, `${tableName || "export"}.csv`),
    type: "csv",
    columns: columns.map((c) => ({ name: c.name, dtype: c.dtype, include: true, cast: "" })),
    dateFormat: "",
    timestampFormat: "",
    delimiter: ",",
    includeHeader: true,
    quoting: "needed",
    compression: defaultCompression,
  };
}

// ---------------------------------------------------------------- expressions

const IDENT = /^[A-Za-z_][A-Za-z0-9_]*$/;

/** DSL column reference: `t[col]`, or `t["col name"]` when the name needs quoting. */
export function columnRef(table: string, column: string): string {
  const col = IDENT.test(column) ? column : `"${column.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
  return `${table}[${col}]`;
}

/** The expression a Math Operation starts with: the selected column, ready
 * to be extended (not the first two columns, which may be an id and a date). */
export function defaultMathExpression(table: string, columns: string[], selected?: string): string {
  return selected && columns.includes(selected) ? columnRef(table, selected) : "";
}

export function formatSignature(f: FunctionDef): string {
  return `${f.name}(${f.params.map((p) => (p.required ? p.name : `[${p.name}]`)).join(", ")})`;
}


export const FUNCTION_CATEGORIES: [string, string[]][] = [
  ["expr.aggregates", ["SUM", "AVG", "COUNT", "COUNTD", "MIN", "MAX"]],
  ["expr.conditionals", ["IF", "COALESCE"]],
  ["expr.text", ["LEN", "UPPER", "LOWER", "TRIM", "CONCAT", "CONTAINS", "STARTSWITH", "ENDSWITH", "REPLACE"]],
  ["expr.numeric", ["ABS", "ROUND", "FLOOR", "CEIL", "POWER", "SQRT"]],
  ["expr.date", ["DATEDIFF", "YEAR", "MONTH", "DAY", "TODAY", "NOW"]],
  ["expr.type_casts", ["CAST", "TRY_CAST"]],
  ["expr.special", ["DROP_DUPLICATES"]],
];

/** Group functions by the help categories; Excel's own functions get a
 * group of their own, anything else goes to "other". */
export function groupFunctions(functions: FunctionDef[], excelNames: string[] = []): [string, FunctionDef[]][] {
  const byName = new Map(functions.map((f) => [f.name.toUpperCase(), f]));
  const used = new Set<string>();
  const groups: [string, FunctionDef[]][] = [];
  for (const [key, names] of FUNCTION_CATEGORIES) {
    const fs = names.map((n) => byName.get(n)).filter((f): f is FunctionDef => !!f);
    fs.forEach((f) => used.add(f.name.toUpperCase()));
    if (fs.length) groups.push([key, fs]);
  }
  const excel = new Set(excelNames.map((n) => n.toUpperCase()));
  const fromExcel = functions.filter((f) => !used.has(f.name.toUpperCase()) && excel.has(f.name.toUpperCase()));
  fromExcel.forEach((f) => used.add(f.name.toUpperCase()));
  if (fromExcel.length) groups.push(["dialog.expr_help.excel_group", fromExcel]);
  const rest = functions.filter((f) => !used.has(f.name.toUpperCase()));
  if (rest.length) groups.push(["dialog.expr_help.other", rest]);
  return groups;
}

/** Excel categories, in the order the reference shows them. */
export const EXCEL_CATEGORIES = ["math", "statistics", "conditional", "logical", "information", "text", "date", "lookup", "tables"];

export interface ExcelRow {
  entry: ExcelEntry;
  /** The ParVu function to write, when there is one. */
  def?: FunctionDef;
}

/** The Excel reference by category, filtered by `query` (name, function, description, alternative). */
export function excelGroups(entries: ExcelEntry[], functions: FunctionDef[], query: string): [string, ExcelRow[]][] {
  const byName = new Map(functions.map((f) => [f.name.toUpperCase(), f]));
  const q = query.trim().toLowerCase();
  const rows: ExcelRow[] = entries
    .map((entry) => ({ entry, def: entry.function ? byName.get(entry.function.toUpperCase()) : undefined }))
    .filter(
      ({ entry, def }) =>
        !q ||
        entry.name.toLowerCase().includes(q) ||
        (entry.function ?? "").toLowerCase().includes(q) ||
        (entry.instead ?? "").toLowerCase().includes(q) ||
        (def?.description ?? "").toLowerCase().includes(q),
    );
  return EXCEL_CATEGORIES.map((c) => [c, rows.filter((r) => r.entry.category === c)] as [string, ExcelRow[]]).filter(
    ([, rs]) => rs.length > 0,
  );
}

export function filterFunctions(functions: FunctionDef[], query: string): FunctionDef[] {
  const q = query.trim().toLowerCase();
  if (!q) return functions;
  return functions.filter(
    (f) =>
      f.name.toLowerCase().includes(q) ||
      f.description.toLowerCase().includes(q) ||
      (f.sql_name ?? "").toLowerCase().includes(q),
  );
}

/** Insert `text` into `value` replacing [start, end); returns the new value and caret. */
export function insertAt(value: string, start: number, end: number, text: string, caretBack = 0): [string, number] {
  const next = value.slice(0, start) + text + value.slice(end);
  return [next, start + text.length - caretBack];
}

// ---------------------------------------------------------------- copy tuple

export function initialSampleSize(defaultSample: number, totalRows: number): number {
  return Math.max(1, Math.min(defaultSample, totalRows || 1, 1000));
}

export function clampInt(v: number, min: number, max: number): number {
  if (!Number.isFinite(v)) return min;
  return Math.min(max, Math.max(min, Math.round(v)));
}

// ---------------------------------------------------------------- settings

export function isTruthySetting(v: unknown): boolean {
  return v === true || v === 1 || (typeof v === "string" && ["true", "1"].includes(v.toLowerCase()));
}

export function toInt(v: unknown, fallback: number): number {
  const n = typeof v === "number" ? v : parseInt(String(v), 10);
  return Number.isFinite(n) ? n : fallback;
}

/** Parse `--name: value;` declarations from a theme's CSS. */
export function parseCssVariables(css: string): Record<string, string> {
  const out: Record<string, string> = {};
  const re = /(--[A-Za-z0-9-]+)\s*:\s*([^;]*);/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(css))) out[m[1]] = m[2].trim();
  return out;
}

export function themeFileName(name: string): string {
  return `${name.toLowerCase().replace(/ /g, "_")}.json`;
}

// ---------------------------------------------------------------- crash report

export function mailtoUrl(email: string, subject: string, body: string, maxBody = 2000): string {
  return `mailto:${email}?subject=${encodeURIComponent(subject)}&body=${encodeURIComponent(body.slice(0, maxBody))}`;
}

// ---------------------------------------------------------------- search

export type SearchParams = Required<Pick<SearchOptions, "query" | "case_sensitive" | "whole_cell" | "regex">> & {
  column: string | null;
};

export type SearchStep =
  | { kind: "hit"; hit: SearchHit; current: number; total: number; more: boolean }
  | { kind: "last"; current: number; total: number }
  | { kind: "first"; total: number }
  | { kind: "none" };

type Finder = (opts: SearchOptions) => Promise<SearchHit | null>;

/**
 * Incremental full-table search with a cache of visited matches, as in
 * search_dialog.py: Next/Prev step through cached matches and only query the
 * backend past either end; the last hit becomes the next anchor.
 */
export class SearchNavigator {
  private cache: SearchHit[] = [];
  private pointer = -1;
  private reachedEnd = false;
  private reachedStart = false;
  private params: string | null = null;

  constructor(private find: Finder) {}

  reset(): void {
    this.cache = [];
    this.pointer = -1;
    this.reachedEnd = false;
    this.reachedStart = false;
    this.params = null;
  }

  private sync(p: SearchParams): void {
    const key = JSON.stringify(p);
    if (key !== this.params) {
      this.reset();
      this.params = key;
    }
  }

  private current(): SearchStep {
    const hit = this.cache[this.pointer];
    const total = this.cache.length;
    return { kind: "hit", hit, current: this.pointer + 1, total, more: this.pointer === total - 1 && !this.reachedEnd };
  }

  async next(p: SearchParams, anchor: { row: number | null; col: number | null }): Promise<SearchStep> {
    this.sync(p);
    if (this.pointer < this.cache.length - 1) {
      this.pointer += 1;
      return this.current();
    }
    if (this.reachedEnd) return { kind: "last", current: this.pointer + 1, total: this.cache.length };
    const a = this.cache.length ? this.cache[this.cache.length - 1] : null;
    const hit = await this.find({
      ...p,
      direction: "next",
      after_row: a ? a.row : anchor.row,
      after_col: a ? a.column_index : anchor.col,
    });
    if (!hit) {
      this.reachedEnd = true;
      return this.cache.length ? { kind: "last", current: this.pointer + 1, total: this.cache.length } : { kind: "none" };
    }
    this.cache.push(hit);
    this.pointer = this.cache.length - 1;
    return this.current();
  }

  async prev(p: SearchParams, anchor: { row: number | null; col: number | null }): Promise<SearchStep> {
    this.sync(p);
    if (this.pointer > 0) {
      this.pointer -= 1;
      return this.current();
    }
    if (this.reachedStart) return { kind: "first", total: this.cache.length };
    const a = this.cache.length ? this.cache[0] : null;
    const hit = await this.find({
      ...p,
      direction: "prev",
      after_row: a ? a.row : anchor.row,
      after_col: a ? a.column_index : anchor.col,
    });
    if (!hit) {
      this.reachedStart = true;
      return this.cache.length ? { kind: "first", total: this.cache.length } : { kind: "none" };
    }
    this.cache.unshift(hit);
    this.pointer = 0;
    return this.current();
  }
}

export function previewValue(value: string, max = 200): string {
  return value.length <= max ? value : value.slice(0, max - 3) + "...";
}

// ---------------------------------------------------------------- reshaping

/** Aggregations that work without a column (they count rows). */
export function aggNeedsColumn(func: AggFunc): boolean {
  return func !== "rows";
}

export function buildGroupByOp(by: string[], aggregations: Aggregation[]): Operation | null {
  const aggs = aggregations
    .filter((a) => !aggNeedsColumn(a.func) || a.column)
    .map((a) => ({ ...a, column: aggNeedsColumn(a.func) ? a.column : "", alias: a.alias?.trim() || null }));
  if (!aggs.length || aggs.length !== aggregations.length) return null;
  return { op: "group_by", by, aggregations: aggs };
}

export function buildPivotOp(index: string[], columns: string, values: string, func: AggFunc): Operation | null {
  if (!columns || index.includes(columns)) return null;
  if (!values && func !== "rows" && func !== "count") return null;
  return { op: "pivot", index, columns, values: values || null, func };
}

export function buildMeltOp(ids: string[], values: string[], variableName: string, valueName: string): Operation | null {
  const v = variableName.trim();
  const n = valueName.trim();
  if (!v || !n || v === n || ids.includes(v) || ids.includes(n)) return null;
  if (values.some((c) => ids.includes(c))) return null;
  return { op: "melt", id_columns: ids, value_columns: values, variable_name: v, value_name: n };
}

export function buildValueCountsOp(columns: string[], normalize: boolean, dropna: boolean): Operation | null {
  return columns.length ? { op: "value_counts", columns, normalize, dropna } : null;
}

// ---------------------------------------------------------------- filter and clean

/** How many values a condition takes: 0 (is null), 1, or 2 (between). */
export function condArity(op: CondOp): 0 | 1 | 2 {
  if (op === "is_null" || op === "not_null" || op === "is_empty" || op === "not_empty") return 0;
  return op === "between" ? 2 : 1;
}

/** Whether the case-sensitivity option applies to this condition. */
export function condIsTextual(op: CondOp): boolean {
  return ["eq", "ne", "contains", "not_contains", "starts_with", "ends_with", "matches", "in", "not_in"].includes(op);
}

/** How a condition's value is typed: it follows the column's type. */
export type ValueKind = "number" | "date" | "datetime" | "time" | "boolean" | "text";

export function valueKind(family: TypeFamily | undefined, dtype = ""): ValueKind {
  switch (family) {
    case "integer":
    case "decimal":
    case "float":
      return "number";
    case "date":
      return "date";
    case "timestamp":
    case "timestamp_tz":
      return "datetime";
    case "time":
      return "time";
    case "boolean":
      return "boolean";
    case undefined:
      return isNumericType(dtype) ? "number" : "text";
    default:
      return "text";
  }
}

const ORDERED_OPS: CondOp[] = ["eq", "ne", "gt", "ge", "lt", "le", "between", "in", "not_in", "is_null", "not_null"];
const TEXT_OPS: CondOp[] = [
  "eq",
  "ne",
  "contains",
  "not_contains",
  "starts_with",
  "ends_with",
  "matches",
  "in",
  "not_in",
  "is_null",
  "not_null",
  "is_empty",
  "not_empty",
];

/** The operators that make sense for a kind of value. */
export function opsFor(kind: ValueKind): CondOp[] {
  if (kind === "text") return TEXT_OPS;
  if (kind === "boolean") return ["eq", "ne", "is_null", "not_null"];
  return ORDERED_OPS;
}

/** The conditions as a row filter, or null while one is incomplete. */
export function buildRowFilter(conditions: Condition[], combine: Combine, isText: (column: string) => boolean): RowFilter | null {
  const op = buildFilterOp(conditions, combine, isText);
  return op && op.op === "filter" ? { conditions: op.conditions, combine: op.combine } : null;
}

/** Set Value: a value (null: empty) on the rows `filter` selects. Text
 * columns may be set to an empty string; other columns need a value. */
export function buildSetValueOp(column: string, value: string | null, isText: boolean, filter: RowFilter | null): Operation | null {
  if (!column || !filter) return null;
  if (value !== null && !isText && !value.trim()) return null;
  return { op: "set_value", column, value, where: filter };
}

export function emptyCondition(column: string): Condition {
  return { column, op: "eq", value: "", value2: "", case_sensitive: false };
}

/**
 * The filter, or null while a condition is incomplete. Comparisons on text
 * columns may compare with an empty string; on other columns a value is needed.
 */
export function buildFilterOp(conditions: Condition[], combine: Combine, isText: (column: string) => boolean): Operation | null {
  if (!conditions.length) return null;
  for (const c of conditions) {
    if (!c.column) return null;
    const arity = condArity(c.op);
    const needsText = !isText(c.column) || ["in", "not_in", "matches", "between"].includes(c.op);
    if (arity >= 1 && needsText && !c.value.trim()) return null;
    if (arity === 2 && !c.value2.trim()) return null;
  }
  return { op: "filter", conditions: conditions.map((c) => ({ ...c })), combine };
}

export function buildFillNullOp(columns: string[], method: FillMethod, value: string): Operation | null {
  if (!columns.length) return null;
  if (method === "value" && value === "") return null;
  return { op: "fill_null", columns, method, value: method === "value" ? value : "" };
}

export function buildSortByOp(keys: SortKey[]): Operation | null {
  const k = keys.filter((x) => x.column);
  const names = k.map((x) => x.column);
  if (!k.length || new Set(names).size !== names.length) return null;
  return { op: "sort_by", keys: k };
}

/** Move the item at `i` by `delta` places, clamped to the list. */
export function moveItem<T>(list: T[], i: number, delta: number): T[] {
  const j = Math.max(0, Math.min(list.length - 1, i + delta));
  if (i === j || i < 0 || i >= list.length) return list;
  const out = list.slice();
  const [x] = out.splice(i, 1);
  out.splice(j, 0, x);
  return out;
}

export function buildSelectColumnsOp(order: string[], kept: Set<string>): Operation | null {
  const columns = order.filter((c) => kept.has(c));
  return columns.length ? { op: "select_columns", columns } : null;
}

export function buildSliceOp(mode: SliceMode, n: number, seed: string): Operation | null {
  if (!Number.isInteger(n) || n < 1) return null;
  const s = seed.trim();
  let parsed: number | null = null;
  if (mode === "sample" && s) {
    if (!/^\d+$/.test(s)) return null;
    parsed = Number(s);
  }
  return { op: "slice", mode, n, seed: parsed };
}

export function buildSplitOp(
  column: string,
  separator: string,
  regex: boolean,
  parts: number | null,
  keepOriginal: boolean,
): Operation | null {
  if (!column || separator === "") return null;
  if (parts !== null && (!Number.isInteger(parts) || parts < 1 || parts > 50)) return null;
  return { op: "split_column", column, separator, regex, parts, keep_original: keepOriginal };
}

/** "0, 10, 2.5" → [0, 10, 2.5]; null when an item is not a number. */
export function parseNumberList(text: string): number[] | null {
  const items = text
    .split(/[,;\s]+/)
    .map((x) => x.trim())
    .filter(Boolean);
  const nums = items.map(Number);
  return nums.every((n) => Number.isFinite(n)) ? nums : null;
}

/** "low, mid, high" → ["low", "mid", "high"]; empty text → []. */
export function parseLabelList(text: string): string[] {
  return text
    .split(",")
    .map((x) => x.trim())
    .filter(Boolean);
}

export function buildBinOp(
  column: string,
  method: BinMethod,
  bins: number,
  edgesText: string,
  labelsText: string,
  output: string,
): Operation | null {
  if (!column || !output.trim()) return null;
  const labels = parseLabelList(labelsText);
  let edges: number[] = [];
  let count = bins;
  if (method === "edges") {
    const e = parseNumberList(edgesText);
    if (!e || new Set(e).size < 2) return null;
    edges = e;
    count = new Set(e).size - 1;
  } else if (!Number.isInteger(bins) || bins < 1 || bins > 1000) {
    return null;
  }
  if (labels.length && labels.length !== count) return null;
  return { op: "bin", column, method, bins: method === "edges" ? 0 : bins, edges, labels, output: output.trim() };
}

// ---------------------------------------------------------------- window functions

export function windowNeedsColumn(func: WindowFunc): boolean {
  return func !== "cum_count" && func !== "row_number";
}

/** Which extra settings a window function uses. */
export function windowUses(func: WindowFunc): { periods: boolean; window: boolean; rank: boolean } {
  return {
    periods: func === "shift" || func === "diff" || func === "pct_change",
    window: func.startsWith("rolling_"),
    rank: func === "rank" || func === "percent_rank",
  };
}

export function defaultWindowOutput(func: WindowFunc, column: string): string {
  const name = func.replace(/^cum_/, "cum");
  return windowNeedsColumn(func) && column ? `${column}_${name}` : name;
}

export function buildWindowOp(spec: WindowSpec): Operation | null {
  if (windowNeedsColumn(spec.func) && !spec.column) return null;
  if (!spec.output.trim()) return null;
  const uses = windowUses(spec.func);
  if (uses.periods && (spec.periods === null || !Number.isInteger(spec.periods) || spec.periods === 0)) return null;
  if (uses.window && (spec.window === null || !Number.isInteger(spec.window) || spec.window < 1)) return null;
  return {
    op: "window",
    ...spec,
    column: windowNeedsColumn(spec.func) ? spec.column : "",
    periods: uses.periods ? spec.periods : null,
    window: uses.window ? spec.window : null,
    order_by: spec.order_by || null,
    output: spec.output.trim(),
  };
}

// ---------------------------------------------------------------- column types

export function isTextType(dtype: string): boolean {
  const t = dtype.toUpperCase();
  return t === "VARCHAR" || t.startsWith("VARCHAR(") || t === "TEXT" || t === "STRING";
}

const NUMERIC_TYPES = [
  "TINYINT",
  "SMALLINT",
  "INTEGER",
  "BIGINT",
  "HUGEINT",
  "UTINYINT",
  "USMALLINT",
  "UINTEGER",
  "UBIGINT",
  "UHUGEINT",
  "FLOAT",
  "DOUBLE",
  "REAL",
];

export function isNumericType(dtype: string): boolean {
  const t = dtype.toUpperCase();
  return NUMERIC_TYPES.includes(t) || t.startsWith("DECIMAL");
}

// ---------------------------------------------------------------- charts

const looksLikeId = (name: string) => /(^id$|_id$|^id_)/i.test(name);

/** Default horizontal axis: the first text-like column, else the first column. */
export function defaultChartX(columns: ColumnMeta[]): string {
  return columns.find((c) => !isNumericType(c.dtype))?.name ?? columns[0]?.name ?? "";
}

/** Default value column: a numeric column other than `x`, preferring ones that are not IDs. */
export function defaultChartY(columns: ColumnMeta[], x: string): string {
  const nums = columns.filter((c) => isNumericType(c.dtype) && c.name !== x).map((c) => c.name);
  return nums.find((n) => !looksLikeId(n)) ?? nums[0] ?? "";
}

// ---------- AI clients (MCP attach) ----------

/** Shell-quote one word for a POSIX shell when it needs it. */
function shellWord(w: string): string {
  return /^[\w@%+=:,./-]+$/.test(w) ? w : `'${w.replace(/'/g, `'\\''`)}'`;
}

/** Setup snippets for connecting AI clients to this ParVu (see McpStatus). */
export function mcpClientSetup(command: string, args: string[], name = "parvu-app") {
  const line = [command, ...args].map(shellWord).join(" ");
  return {
    claudeCode: `claude mcp add -s user ${name} -- ${line}`,
    claudeDesktop: JSON.stringify({ mcpServers: { [name]: { command, args } } }, null, 2),
    plain: line,
  };
}

// ---------- Convert ----------

/** The Convert operation, or null while a replacement value is chosen but empty. */
export function buildConvertOp(column: string, to: Reading, onError: string | null, keepOriginal: boolean): Operation | null {
  if (onError !== null && !onError.trim()) return null;
  return { op: "convert", column, to, on_error: onError === null ? null : onError.trim(), keep_original: keepOriginal };
}

/** Text key (and args) describing an error count, or null while counting. */
export function conversionSummary(
  count: { errors: number; checked: number; complete: boolean } | null,
): { key: string; args: Record<string, string> } | null {
  if (!count) return null;
  const n = (v: number) => v.toLocaleString("en-US");
  const args = { errors: n(count.errors), checked: n(count.checked) };
  if (count.errors === 0) return { key: count.complete ? "dialog.convert.no_errors_all" : "dialog.convert.no_errors_sample", args };
  return { key: count.complete ? "dialog.convert.errors_all" : "dialog.convert.errors_sample", args };
}

// ---------- Open with options ----------

export interface OpenForm {
  /** "auto", a delimiter, or "custom" (then `custom`). */
  delimiter: string;
  custom: string;
  header: "auto" | "yes" | "no";
  /** Rows to skip; empty: automatic. */
  skip: string;
  /** "auto" (a point) or ",". */
  decimal: string;
  /** "auto" (UTF-8) or an encoding. */
  encoding: string;
}

/** The parsing options of the form, or null while a field is invalid. */
export function csvOptionsFrom(f: OpenForm): CsvOptions | null {
  const delimiter = f.delimiter === "auto" ? null : f.delimiter === "custom" ? f.custom : f.delimiter;
  if (delimiter !== null && (delimiter.length === 0 || delimiter.length > 4)) return null;
  let skip: number | null = null;
  if (f.skip.trim()) {
    skip = Number(f.skip);
    if (!Number.isInteger(skip) || skip < 0) return null;
  }
  return {
    delimiter,
    header: f.header === "auto" ? null : f.header === "yes",
    skip,
    decimal_separator: f.decimal === "auto" ? null : f.decimal,
    encoding: f.encoding === "auto" ? null : f.encoding,
  };
}

/** A delimiter for people: "Semicolon (;)", "Tab". */
export function delimiterLabel(d: string, t: Translate): string {
  const named: Record<string, string> = { ",": "comma", ";": "semicolon", "\t": "tab", "|": "pipe", " ": "space" };
  return named[d] ? t(`open_options.delim.${named[d]}`) : d;
}

// ---------------------------------------------------------------- custom tools

/** The type sets a signature may use and their families (mirrors the
 * backend's `custom::signature::TYPE_SETS`). */
export const TYPE_SET_FAMILIES: Record<string, TypeFamily[]> = {
  any: [
    "integer", "decimal", "float", "text", "boolean", "date", "timestamp", "timestamp_tz", "time", "interval",
    "json", "list", "struct", "map", "binary", "other",
  ],
  number: ["integer", "decimal", "float"],
  integer: ["integer"],
  decimal: ["decimal"],
  float: ["float"],
  text: ["text"],
  boolean: ["boolean"],
  date: ["date"],
  datetime: ["timestamp", "timestamp_tz"],
  date_or_datetime: ["date", "timestamp", "timestamp_tz"],
  time: ["time"],
  interval: ["interval"],
  json: ["json"],
  list: ["list"],
  struct: ["struct"],
};

/** Families of a list of type set names; empty means any. */
export function familiesOf(types?: string[]): TypeFamily[] {
  if (!types?.length) return TYPE_SET_FAMILIES.any;
  const out: TypeFamily[] = [];
  for (const n of types) for (const f of TYPE_SET_FAMILIES[n] ?? []) if (!out.includes(f)) out.push(f);
  return out;
}

/** The columns a column parameter accepts. */
export function fittingColumns(param: CustomParam, columns: ColumnMeta[]): ColumnMeta[] {
  const fams = familiesOf(param.type);
  return columns.filter((c) => fams.includes(c.family ?? "other"));
}

const typeText = (types?: string[]) => (types?.length ? types.join(" | ") : "any");

/** `clean_phone(phone: column<text>, prefix: value<VARCHAR> = "+") → column<text>` */
export function signatureText(def: CustomDefinition): string {
  const params = def.params.map((p) => {
    if (p.kind === "table") {
      const cols = (p.columns ?? []).map((c) => `${c.name}: ${typeText(c.type)}`);
      return cols.length ? `${p.name}: table{${cols.join(", ")}}` : `${p.name}: table`;
    }
    const ty = p.kind === "column" ? `column<${typeText(p.type)}>` : `value<${p.dtype ?? "VARCHAR"}>`;
    const dflt = p.optional ? (p.default !== undefined && p.default !== null ? ` = ${JSON.stringify(p.default)}` : " = empty") : "";
    return `${p.name}: ${ty}${dflt}`;
  });
  const r = def.result;
  const result =
    r.kind === "table"
      ? "table"
      : `${r.kind}<${r.same_as ? `same as ${r.same_as}` : typeText(r.type)}>`;
  return `${def.name}(${params.join(", ")}) → ${result}`;
}

/** Usable definitions (validated), and the others to show disabled. */
export function customDefinitions(entries: CustomEntry[]): CustomDefinition[] {
  return entries.flatMap((e) => (e.definition ? [e.definition] : []));
}

export const isUsable = (d: CustomDefinition) => d.status === "ok" && !!d.compiled;

/** The functions offered for a column: those whose first column parameter
 * accepts its family. */
export function functionsForColumn(defs: CustomDefinition[], family: TypeFamily | undefined): CustomDefinition[] {
  return defs.filter((d) => {
    if (d.kind !== "function") return false;
    const first = d.params.find((p) => p.kind === "column");
    return !!first && (!family || familiesOf(first.type).includes(family));
  });
}

/** Operations by where they put their result. */
export function operationsFor(defs: CustomDefinition[], newTable: boolean): CustomDefinition[] {
  return defs.filter((d) => d.kind === "operation" && isNewTable(d) === newTable);
}

export function isNewTable(d: CustomDefinition): boolean {
  return d.params.filter((p) => p.kind === "table").length > 1 || d.placement === "new_table";
}

/** What the apply dialog collects. Column and table arguments are names;
 * values are text ("" = not given). */
export interface CustomForm {
  args: Record<string, string>;
  toNew: boolean;
  newName: string;
}

/** Defaults: the selected column for the first column parameter that fits
 * it, the first fitting column for the others, value defaults. */
export function initialCustomForm(def: CustomDefinition, columns: ColumnMeta[], selected?: string, others: { name: string; columns: ColumnMeta[] }[] = []): CustomForm {
  const args: Record<string, string> = {};
  const tables = def.params.filter((p) => p.kind === "table");
  const used = new Set<string>();
  tables.slice(1).forEach((t, i) => (args[t.name] = others[i]?.name ?? others[0]?.name ?? ""));
  for (const p of def.params) {
    if (p.kind === "column") {
      const cols = columnsFor(def, p, columns, args, others);
      const fits = fittingColumns(p, cols);
      const pick = fits.find((c) => c.name === selected && !used.has(c.name)) ?? fits.find((c) => !used.has(c.name)) ?? fits[0];
      args[p.name] = pick?.name ?? "";
      if (pick) used.add(pick.name);
    } else if (p.kind === "value") {
      args[p.name] = p.default ?? "";
    }
  }
  return { args, toNew: true, newName: "" };
}

/** The columns a column parameter picks from: this table's, or those of
 * the other table it belongs to. */
export function columnsFor(
  def: CustomDefinition,
  p: CustomParam,
  columns: ColumnMeta[],
  args: Record<string, string>,
  others: { name: string; columns: ColumnMeta[] }[],
): ColumnMeta[] {
  if (def.kind !== "operation") return columns;
  const tables = def.params.filter((x) => x.kind === "table");
  const of = p.of ? tables.find((t) => t.name === p.of) : tables[0];
  if (!of || of === tables[0]) return columns;
  return others.find((o) => o.name === args[of.name])?.columns ?? [];
}

/** Problems with the form, as `[parameter, translation key]`. */
export function customArgProblems(
  def: CustomDefinition,
  columns: ColumnMeta[],
  form: CustomForm,
  others: { name: string; columns: ColumnMeta[] }[] = [],
): [string, string][] {
  const out: [string, string][] = [];
  const tables = def.params.filter((p) => p.kind === "table");
  for (const p of def.params) {
    const v = form.args[p.name] ?? "";
    if (p.kind === "table") {
      if (p === tables[0]) continue;
      const t = others.find((o) => o.name === v);
      if (!t) out.push([p.name, "custom.apply.choose_table"]);
      else if ((p.columns ?? []).some((rc) => !t.columns.some((c) => c.name === rc.name))) out.push([p.name, "custom.apply.missing_columns"]);
    } else if (p.kind === "column") {
      const meta = columnsFor(def, p, columns, form.args, others).find((c) => c.name === v);
      if (!v || !meta) out.push([p.name, "custom.apply.choose_column"]);
      else if (!familiesOf(p.type).includes(meta.family ?? "other")) out.push([p.name, "custom.apply.wrong_type"]);
    } else {
      if (!v.trim()) {
        if (!p.optional) out.push([p.name, "custom.apply.enter_value"]);
        continue;
      }
      if (p.choices?.length && !p.choices.includes(v)) out.push([p.name, "custom.apply.not_a_choice"]);
      const fam = familyOfType(p.dtype ?? "VARCHAR");
      if (fam === "integer" || fam === "decimal" || fam === "float") {
        const n = Number(v);
        if (!Number.isFinite(n) || (fam === "integer" && !Number.isInteger(n))) out.push([p.name, "custom.apply.not_a_number"]);
        else if ((p.min !== undefined && p.min !== null && n < p.min) || (p.max !== undefined && p.max !== null && n > p.max))
          out.push([p.name, "custom.apply.out_of_range"]);
      }
    }
  }
  if (def.kind === "function" && def.result.kind === "column" && form.toNew && form.newName.trim() && columns.some((c) => c.name === form.newName.trim()))
    out.push(["", "custom.apply.name_taken"]);
  return out;
}

/** The family of a DuckDB type name (a small version of the backend's `TypeFamily::of`). */
export function familyOfType(dtype: string): TypeFamily {
  const t = dtype.trim().toUpperCase();
  if (t.endsWith("]")) return "list";
  const base = t.split("(")[0].trim();
  if (["TINYINT", "SMALLINT", "INTEGER", "INT", "BIGINT", "HUGEINT", "UBIGINT", "UINTEGER"].includes(base)) return "integer";
  if (base === "DECIMAL" || base === "NUMERIC") return "decimal";
  if (["FLOAT", "DOUBLE", "REAL"].includes(base)) return "float";
  if (["VARCHAR", "TEXT", "STRING"].includes(base)) return "text";
  if (base === "BOOLEAN" || base === "BOOL") return "boolean";
  if (base === "DATE") return "date";
  if (base.startsWith("TIMESTAMP WITH") || base === "TIMESTAMPTZ") return "timestamp_tz";
  if (base.startsWith("TIMESTAMP") || base === "DATETIME") return "timestamp";
  if (base === "TIME") return "time";
  if (base === "INTERVAL") return "interval";
  if (base === "JSON") return "json";
  if (base === "STRUCT") return "struct";
  return "other";
}

/** The arguments to send: columns, values given (empty optional values
 * are left out so the default applies), other tables. */
export function customArgs(def: CustomDefinition, form: CustomForm): Record<string, CustomArg> {
  const out: Record<string, CustomArg> = {};
  const tables = def.params.filter((p) => p.kind === "table");
  for (const p of def.params) {
    const v = form.args[p.name] ?? "";
    if (p.kind === "column") out[p.name] = { column: v };
    else if (p.kind === "table") {
      if (p !== tables[0]) out[p.name] = { table: v };
    } else if (v.trim() !== "" || !p.optional) out[p.name] = { value: v };
  }
  return out;
}

/** The operation the apply dialog returns, or null while the form has a
 * problem. Summary functions are not operations (their value is shown). */
export function buildCustomOp(
  def: CustomDefinition,
  form: CustomForm,
  columns: ColumnMeta[],
  others: { name: string; columns: ColumnMeta[] }[] = [],
  filter?: RowFilter,
): Operation | null {
  if (customArgProblems(def, columns, form, others).length) return null;
  const args = customArgs(def, form);
  if (def.kind === "operation") return { op: "custom_operation", name: def.name, args, new_table: isNewTable(def) };
  if (def.result.kind !== "column") return null;
  const output: FnOutput = form.toNew ? { to: "new", name: form.newName.trim() } : { to: "in_place" };
  return { op: "custom_function", name: def.name, args, output, ...(filter ? { where: filter } : {}) };
}

/** A DuckDB type for an example column of a parameter's type. */
export function exampleDtype(types?: string[]): string {
  const f = familiesOf(types)[0];
  const map: Partial<Record<TypeFamily, string>> = {
    integer: "BIGINT", decimal: "DECIMAL(18,3)", float: "DOUBLE", text: "VARCHAR", boolean: "BOOLEAN", date: "DATE",
    timestamp: "TIMESTAMP", timestamp_tz: "TIMESTAMPTZ", time: "TIME", interval: "INTERVAL", json: "JSON",
  };
  return types?.length && !types.includes("any") ? (map[f] ?? "VARCHAR") : "VARCHAR";
}

/** Types offered for example columns. */
export const EXAMPLE_TYPES = ["VARCHAR", "BIGINT", "DOUBLE", "DECIMAL(18,3)", "BOOLEAN", "DATE", "TIMESTAMP", "TIME", "JSON"];

/** A function example's input has one column per column parameter, named
 * after it: keep it in step with the signature (types and rows kept). */
export function syncFunctionExample(def: CustomDefinition, ex: CustomExample): CustomExample {
  if (def.kind !== "function") return ex;
  const params = def.params.filter((p) => p.kind === "column");
  const old = ex.input.columns;
  const columns = params.map((p) => old.find((c) => c.name === p.name) ?? { name: p.name, dtype: exampleDtype(p.type) });
  const rows = ex.input.rows.map((r) => columns.map((c) => {
    const i = old.findIndex((o) => o.name === c.name);
    return i >= 0 ? (r[i] ?? null) : null;
  }));
  const summary = def.result.kind === "value";
  let expected = ex.expected.rows;
  if (summary) expected = [expected[0] ?? [null]];
  else expected = rows.map((_, i) => expected[i] ?? [null]);
  return { ...ex, input: { columns, rows }, expected: { columns: [{ name: "result" }], rows: expected } };
}

/** A new, empty example for `def`. */
export function newExample(def: CustomDefinition, n: number): CustomExample {
  if (def.kind === "function") {
    return syncFunctionExample(def, { name: `example ${n}`, input: { columns: [], rows: [[]] }, expected: { columns: [{ name: "result" }], rows: [[null]] }, args: {} });
  }
  const cols = def.params.filter((p) => p.kind === "column").map((p) => ({ name: p.name, dtype: exampleDtype(p.type) }));
  const input: ExampleTable = { columns: cols.length ? cols : [{ name: "a", dtype: "VARCHAR" }], rows: [] };
  input.rows = [input.columns.map(() => null)];
  const args: Record<string, Cell> = {};
  for (const p of def.params) if (p.kind === "column") args[p.name] = p.name;
  return { name: `example ${n}`, input, args, expected: { columns: input.columns.map((c) => ({ name: c.name })), rows: [] } };
}

/** A definition to start from. */
export function newDefinition(kind: "function" | "operation"): CustomDefinition {
  const def: CustomDefinition =
    kind === "function"
      ? {
          schema: 1, kind, name: "", status: "draft",
          params: [{ name: "x", kind: "column", type: ["number"] }],
          result: { kind: "column", type: ["number"] },
          body: { language: "sql", source: "x * 2" },
          flags: { uses_current_time: false },
          examples: [],
        }
      : {
          schema: 1, kind, name: "", status: "draft",
          params: [{ name: "t", kind: "table" }, { name: "x", kind: "column", type: ["number"] }],
          result: { kind: "table" },
          placement: "step",
          body: { language: "sql", source: "SELECT * FROM t\nWHERE x > 0" },
          flags: { uses_current_time: false },
          examples: [],
        };
  def.examples = [newExample(def, 1)];
  return def;
}

/** A cell typed in an example grid: empty is "no value"; `""` is empty text. */
export function cellFromInput(text: string): Cell {
  if (text === "") return null;
  if (text === '""') return "";
  return text;
}

export function cellToInput(c: Cell): string {
  if (c === null) return "";
  if (c === "") return '""';
  return c;
}

/** The expected table filled from what the body gave. */
export function expectedFromActual(def: CustomDefinition, actual: ExampleTable): ExampleTable {
  if (def.kind === "function") return { columns: [{ name: "result" }], rows: actual.rows.map((r) => [r[0] ?? null]) };
  return { columns: actual.columns.map((c) => ({ name: c.name })), rows: actual.rows.map((r) => [...r]) };
}

/** A copy with a free name (`name_copy`, `name_copy_2`…). */
export function duplicateDefinition(def: CustomDefinition, taken: string[]): CustomDefinition {
  let name = `${def.name}_copy`.slice(0, 40);
  for (let i = 2; taken.includes(name); i++) name = `${def.name}_copy_${i}`.slice(0, 40);
  const copy: CustomDefinition = JSON.parse(JSON.stringify(def));
  return { ...copy, name, sample: false, author: "", status: "draft", compiled: undefined, report: undefined, created: undefined, modified: undefined };
}

/** What the editor sends: drops the report and compiled form, trims names. */
export function definitionToSave(def: CustomDefinition): CustomDefinition {
  const d: CustomDefinition = JSON.parse(JSON.stringify(def));
  d.name = d.name.trim();
  d.compiled = undefined;
  d.report = undefined;
  for (const p of d.params) {
    p.name = p.name.trim();
    if (p.kind !== "value") {
      delete p.dtype;
      delete p.default;
      delete p.optional;
    }
    if (p.kind !== "column") delete p.type;
    if (p.kind === "value" && !p.optional) delete p.default;
  }
  if (d.kind === "function") delete d.placement;
  return d;
}

/** Menu entries collapse past this many. */
export const CUSTOM_MENU_MAX = 12;
