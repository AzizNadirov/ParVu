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
  SliceMode,
  SortKey,
  WindowFunc,
  WindowSpec,
  ColumnSpec,
  ColumnStats,
  ExportFormat,
  ExportOptions,
  FunctionDef,
  JoinType,
  Keep,
  Operation,
  Quoting,
  ReplaceRequest,
  SearchHit,
  SearchOptions,
} from "../lib/api";

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
export function sortUniqueValues(values: Cell[]): Cell[] {
  const nulls = values.filter((v) => v === null);
  const rest = values.filter((v): v is string => v !== null);
  rest.sort((a, b) => {
    const la = a.toLowerCase();
    const lb = b.toLowerCase();
    return la < lb ? -1 : la > lb ? 1 : a < b ? -1 : a > b ? 1 : 0;
  });
  return [...nulls, ...rest];
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
    ["column_stats.distinct", fmtNum(s.distinct)],
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

export function defaultMathExpression(table: string, columns: string[]): string {
  return columns.length >= 2 ? `${columnRef(table, columns[0])} + ${columnRef(table, columns[1])}` : "";
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

/** Group functions by the help categories; anything not listed goes to "other". */
export function groupFunctions(functions: FunctionDef[]): [string, FunctionDef[]][] {
  const byName = new Map(functions.map((f) => [f.name.toUpperCase(), f]));
  const used = new Set<string>();
  const groups: [string, FunctionDef[]][] = [];
  for (const [key, names] of FUNCTION_CATEGORIES) {
    const fs = names.map((n) => byName.get(n)).filter((f): f is FunctionDef => !!f);
    fs.forEach((f) => used.add(f.name.toUpperCase()));
    if (fs.length) groups.push([key, fs]);
  }
  const rest = functions.filter((f) => !used.has(f.name.toUpperCase()));
  if (rest.length) groups.push(["dialog.expr_help.other", rest]);
  return groups;
}

export function filterFunctions(functions: FunctionDef[], query: string): FunctionDef[] {
  const q = query.trim().toLowerCase();
  if (!q) return functions;
  return functions.filter(
    (f) =>
      f.name.toLowerCase().includes(q) ||
      f.description.toLowerCase().includes(q) ||
      f.sql_name.toLowerCase().includes(q),
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

export function validTableVar(name: string, keywords: string[]): boolean {
  const n = name.trim();
  return !!n && !keywords.map((k) => k.toUpperCase()).includes(n.toUpperCase());
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
