// Typed wrappers around the Tauri commands in crates/parvu-app/src/commands.rs.
// Every call rejects with an ApiError.
import { invoke } from "@tauri-apps/api/core";
import { startJob, type JobOptions } from "./jobs";

export type Cell = string | null;

/** What kind of values a column holds (from the backend's `TypeFamily`). */
export type TypeFamily =
  | "integer"
  | "decimal"
  | "float"
  | "text"
  | "boolean"
  | "date"
  | "timestamp"
  | "timestamp_tz"
  | "time"
  | "interval"
  | "json"
  | "list"
  | "struct"
  | "map"
  | "binary"
  | "other";

export type Side = "both" | "start" | "end";
export type MatchHow = "contains" | "starts_with" | "ends_with" | "regex";

/** How to parse a CSV file ("Open with options"); null lets DuckDB detect it. */
export interface CsvOptions {
  delimiter: string | null;
  header: boolean | null;
  skip: number | null;
  decimal_separator: string | null;
  encoding: string | null;
}

/** First rows of a CSV file with options, and what DuckDB detects on its own. */
export interface CsvPreview {
  columns: ColumnMeta[];
  rows: Cell[][];
  detected: CsvOptions | null;
}

export type TimeUnitName = "year" | "quarter" | "month" | "week" | "day" | "hour" | "minute" | "second";
export type DatePartName = "year" | "quarter" | "month" | "day" | "day_of_year" | "day_of_week" | "week" | "hour" | "minute" | "second";

/** A range of dates relative to today (backend `RelativeRange`). */
export type RelativeRange =
  | { kind: "last"; n: number; unit: TimeUnitName }
  | { kind: "this"; unit: TimeUnitName }
  | { kind: "previous"; unit: TimeUnitName }
  | { kind: "year_to_date" };

/** A function of one column (backend `ColumnFn`). */
export type ColumnFn =
  | { fn: "trim"; side: Side }
  | { fn: "clean" }
  | { fn: "collapse_spaces" }
  | { fn: "lower" }
  | { fn: "upper" }
  | { fn: "proper" }
  | { fn: "remove_accents" }
  | { fn: "add_prefix"; text: string }
  | { fn: "add_suffix"; text: string }
  | { fn: "pad"; length: number; fill: string; side: Side }
  | { fn: "length" }
  | { fn: "first"; n: number }
  | { fn: "last"; n: number }
  | { fn: "range"; start: number; length: number }
  | { fn: "before"; delimiter: string; last: boolean }
  | { fn: "after"; delimiter: string; last: boolean }
  | { fn: "between"; start: string; end: string }
  | { fn: "regex_extract"; pattern: string; group: number }
  | { fn: "invert" }
  | { fn: "date_part"; part: DatePartName }
  | { fn: "month_name"; names: string[] }
  | { fn: "day_name"; names: string[] }
  | { fn: "start_of"; unit: TimeUnitName }
  | { fn: "end_of"; unit: TimeUnitName }
  | { fn: "date_only" }
  | { fn: "time_only" }
  | { fn: "age"; unit: TimeUnitName }
  | { fn: "add_time"; n: number; unit: TimeUnitName }
  | { fn: "format_date"; format: string }
  | { fn: "is_weekend" }
  | { fn: "matches"; text: string; how: MatchHow; ignore_case: boolean }
  | { fn: "json_path"; path: string; text: boolean }
  | { fn: "list_length" }
  | { fn: "join_list"; separator: string };

/** A key of a JSON object column and the type its values get. */
export interface JsonKey {
  name: string;
  dtype: string;
}

/** Where a column function writes. */
export type FnOutput = { to: "in_place" } | { to: "new"; name: string };

/** How a column of a CSV file is turned into its type (backend `Reading`). */
export type Reading =
  | { kind: "text" }
  | { kind: "integer" }
  | { kind: "number" }
  | { kind: "locale_number"; thousands: string; decimal: string }
  | { kind: "boolean" }
  | { kind: "date" }
  | { kind: "date_format"; format: string }
  | { kind: "timestamp" }
  | { kind: "timestamp_format"; format: string }
  | { kind: "time" }
  | { kind: "json" }
  | { kind: "cast"; dtype: string }
  | { kind: "epoch"; unit: "seconds" | "milliseconds" | "microseconds" };

/** A column of a CSV file and how it is read. */
export interface SourceColumn {
  name: string;
  reading: Reading;
  /** Read by DuckDB's own typed reader. */
  native: boolean;
  /** Day and month order was a guess (e.g. 03/04/2024). */
  ambiguous: boolean;
}

export interface ColumnMeta {
  name: string;
  dtype: string;
  /** Sent by the backend for every column; optional for hand-made metadata. */
  family?: TypeFamily;
}

export interface Page {
  page: number;
  total_pages: number;
  total_rows: number;
  page_size: number;
  offset: number;
  columns: ColumnMeta[];
  rows: Cell[][];
  /** [row_in_page, column_index] of cells showing a pending edit. */
  edited: [number, number][];
  editable: boolean;
}

export interface StepLabel {
  key: string;
  args: Record<string, string>;
}

export type FileFormat = "parquet" | "csv" | "json" | "excel";

export interface TabSummary {
  id: number;
  name: string;
  path: string;
  format: FileFormat;
  file_size: number;
  total_rows: number;
  total_pages: number;
  page_size: number;
  is_base_query: boolean;
  can_undo: boolean;
  current_query: string;
  steps: StepLabel[];
  pending_edits: number;
  /** Column formulas of the table (live computed columns). */
  formulas: Formula[];
  /** The worksheet shown, for Excel workbooks (its name after saving). */
  sheet: string | null;
  /** Sheets were renamed, hidden, deleted… and not saved yet. */
  sheets_changed: boolean;
  /** A table operation's result not saved to a file yet (its `path` is empty). */
  origin: Origin | null;
}

/** How a result tab was made: a table operation or a SQL query. */
export type Derivation = { kind: "op"; op: Operation } | { kind: "sql"; sql: string };

/** Where a result tab came from. */
export interface Origin {
  source_id: number;
  source: string;
  label: StepLabel;
  derivation: Derivation;
}

/** What making a result tab did. */
export type MadeTable = { kind: "opened"; summary: TabSummary } | { kind: "too_big"; limit: number };

/** Operations whose result is a different table: they open a new tab. */
export const TABLE_OPS = ["group_by", "pivot", "melt", "transpose", "value_counts", "describe", "join", "append"] as const;
export const isTableOp = (op: Operation): boolean => (TABLE_OPS as readonly string[]).includes(op.op);

/** A column formula: the column it makes, what the user typed, and where it sits. */
/** A sheet of a workbook as it will be saved. */
export interface PlanSheet {
  name: string;
  /** Its name in the file now. */
  source: string;
  hidden: boolean;
  deleted: boolean;
  /** A duplicate made in ParVu, not saved yet (no data of its own in the file). */
  copy?: boolean;
}

export interface SheetsView {
  /** The sheet this tab shows. */
  current: string;
  sheets: PlanSheet[];
  /** Sheets (stored names) open in a tab of this window. */
  open: string[];
  changed: boolean;
}

export type SheetOp =
  | { op: "rename"; sheet: string; name: string }
  | { op: "hide" | "show" | "delete" | "restore" | "duplicate"; sheet: string }
  | { op: "move"; sheet: string; by: number };

export interface Formula {
  name: string;
  source: string;
  sql: string;
  after: string | null;
  /** Column totals it uses (aggregate SQL), computed once per rebuild. */
  totals?: string[];
}

/** Which of a group of duplicates is kept ("none": every one goes, pandas keep=False). */
export type Keep = "first" | "last" | "none";
export type JoinType = "inner" | "left" | "right" | "full_outer";

export type Operation =
  | { op: "sort"; column: string; ascending: boolean }
  | { op: "rename"; column: string; new_name: string }
  | { op: "remove"; column: string }
  | { op: "change_type"; column: string; new_type: string }
  | { op: "convert"; column: string; to: Reading; on_error: string | null; keep_original: boolean }
  | { op: "column_transform"; column: string; func: ColumnFn; output: FnOutput; where?: RowFilter }
  | { op: "merge_columns"; columns: string[]; separator: string; name: string; keep_columns: boolean }
  | { op: "date_diff"; start: string; end: string; unit: TimeUnitName; name: string }
  | { op: "expand_json"; column: string; keys: JsonKey[]; keep_original: boolean }
  | { op: "expand_struct"; column: string }
  | { op: "to_rows"; column: string }
  | { op: "split_to_rows"; column: string; delimiter: string; trim: boolean }
  | { op: "date_range"; column: string; range: RelativeRange }
  | { op: "conversion_errors"; column: string; to: Reading; keep: boolean }
  | { op: "duplicate"; column: string }
  | { op: "drop_duplicates"; columns: string[]; keep: Keep }
  | { op: "mark_duplicates"; columns: string[]; keep: Keep; name: string }
  | { op: "drop_null"; column: string; null_value: string | null }
  | { op: "filter_values"; column: string; values: Cell[] }
  | { op: "join"; right: string; join_type: JoinType; left_key: string; right_key: string }
  | { op: "append"; tables: string[]; distinct: boolean }
  | { op: "add_column"; name: string; expression: string }
  // reshaping
  | { op: "group_by"; by: string[]; aggregations: Aggregation[] }
  | { op: "pivot"; index: string[]; columns: string; values: string | null; func: AggFunc }
  | { op: "melt"; id_columns: string[]; value_columns: string[]; variable_name: string; value_name: string }
  | { op: "transpose"; header_column: string | null }
  | { op: "value_counts"; columns: string[]; normalize: boolean; dropna: boolean }
  | { op: "describe" }
  // filtering and cleaning
  | { op: "filter"; conditions: Condition[]; combine: Combine }
  | { op: "fill_null"; columns: string[]; method: FillMethod; value: string; where?: RowFilter }
  | { op: "set_value"; column: string; value: string | null; where: RowFilter }
  | { op: "sort_by"; keys: SortKey[] }
  | { op: "select_columns"; columns: string[] }
  | { op: "slice"; mode: SliceMode; n: number; seed: number | null }
  | { op: "split_column"; column: string; separator: string; regex: boolean; parts: number | null; keep_original: boolean }
  | {
      op: "bin";
      column: string;
      method: BinMethod;
      bins: number;
      edges: number[];
      labels: string[];
      output: string;
    }
  // column functions
  | ({ op: "window" } & WindowSpec);

export type ChartKind = "bar" | "line" | "area" | "scatter" | "histogram";
export type TimeUnit = "auto" | "none" | "day" | "week" | "month" | "quarter" | "year";
export type ChartSort = "auto" | "value_desc" | "value_asc" | "label";
export interface ChartSpec {
  kind: ChartKind;
  x: string;
  y: string | null;
  agg: AggFunc;
  /** Column that splits the values into series. */
  series: string | null;
  time_unit: TimeUnit;
  sort: ChartSort;
  /** Categories shown (default 20). */
  limit: number | null;
  /** Fold the categories past `limit` into "Other". */
  other: boolean;
  bins: number | null;
}
export interface ChartSeries {
  /** Value of the split column; "" when the chart isn't split. */
  name: string;
  other: boolean;
  values: (number | null)[];
  /** Scatter only. */
  xs: (number | null)[];
}
export interface ChartData {
  x_kind: "category" | "time" | "number";
  time_unit: TimeUnit | null;
  labels: string[];
  other_label: number | null;
  series: ChartSeries[];
  bin_edges: number[];
  truncated: boolean;
}

export const AGG_FUNCS = [
  "count",
  "rows",
  "count_distinct",
  "sum",
  "mean",
  "median",
  "min",
  "max",
  "std",
  "var",
  "first",
  "last",
  "mode",
  "list",
] as const;
export type AggFunc = (typeof AGG_FUNCS)[number];
export interface Aggregation {
  column: string;
  func: AggFunc;
  alias: string | null;
}

export const COND_OPS = [
  "eq",
  "ne",
  "gt",
  "ge",
  "lt",
  "le",
  "between",
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
] as const;
export type CondOp = (typeof COND_OPS)[number];
export interface Condition {
  column: string;
  op: CondOp;
  value: string;
  value2: string;
  case_sensitive: boolean;
}
export type Combine = "and" | "or";

export const FILL_METHODS = ["value", "forward", "backward", "mean", "median", "mode"] as const;
export type FillMethod = (typeof FILL_METHODS)[number];

export interface SortKey {
  column: string;
  ascending: boolean;
  nulls_first: boolean;
}
export type SliceMode = "head" | "tail" | "sample";
export type BinMethod = "equal_width" | "quantile" | "edges";

export const WINDOW_FUNCS = [
  "cum_sum",
  "cum_min",
  "cum_max",
  "cum_mean",
  "cum_count",
  "shift",
  "diff",
  "pct_change",
  "rank",
  "percent_rank",
  "row_number",
  "rolling_mean",
  "rolling_sum",
  "rolling_min",
  "rolling_max",
  "rolling_std",
] as const;
export type WindowFunc = (typeof WINDOW_FUNCS)[number];
export type RankMethod = "average" | "min" | "max" | "dense" | "first";
export interface WindowSpec {
  column: string;
  func: WindowFunc;
  periods: number | null;
  window: number | null;
  rank_method: RankMethod;
  ascending: boolean;
  partition_by: string[];
  order_by: string | null;
  output: string;
}

export interface ReplaceRequest {
  column: string;
  pattern: string;
  replacement: string;
  output_column: string;
  case_sensitive: boolean;
  regex: boolean;
  /** Only replace in these rows. */
  where?: RowFilter;
}

/** Which rows an operation changes (pandas `df.loc[condition, column]`). */
export interface RowFilter {
  conditions: Condition[];
  combine: Combine;
}

/** Unique values of a column; `complete` when every row was read. */
export interface UniqueValues {
  values: Cell[];
  complete: boolean;
  stopped: "too_many" | "too_large" | null;
  rows_scanned: number;
}

export interface ColumnStats {
  column: string;
  dtype: string;
  row_count: number;
  non_null: number;
  null: number;
  distinct: number;
  /** `distinct` is an estimate (a big table). */
  distinct_approx?: boolean;
  min: string | null;
  max: string | null;
  mean: number | null;
  std: number | null;
}

export interface SearchOptions {
  query: string;
  column?: string | null;
  after_row?: number | null;
  after_col?: number | null;
  direction?: "next" | "prev";
  case_sensitive?: boolean;
  whole_cell?: boolean;
  regex?: boolean;
}

export interface SearchHit {
  row: number;
  column_index: number;
  column: string;
  value: string;
}

export type ExportFormat = "csv" | "parquet" | "json" | "excel";
export type Quoting = "needed" | "all_valid" | "none";

export interface ColumnSpec {
  name: string;
  source_type: string;
  include: boolean;
  cast_to: string;
}

export interface ExportOptions {
  path: string;
  format: ExportFormat;
  columns: ColumnSpec[];
  date_format: string;
  timestamp_format: string;
  delimiter: string;
  include_header: boolean;
  quoting: Quoting;
  compression: string;
  json_lines: boolean;
}

export interface ExportProgress {
  job: number;
  percent: number | null;
}

export interface ExportFinished {
  job: number;
  ok: boolean;
  cancelled: boolean;
  rows: number;
  path: string;
  error: string | null;
}

export type WarningCriteria = "rows" | "cells" | "filesize";

/** Settings as stored in settings.json. Some numbers are stored as strings. */
export interface Settings {
  default_data_var_name: string;
  default_limit: number | string;
  default_sql_font_size: number | string;
  default_result_font_size: number | string;
  default_sql_query: string;
  default_sql_font: string;
  sql_keywords: string[];
  result_pagination_rows_per_page: number | string;
  save_file_history: string;
  max_rows: number | string;
  current_theme: string;
  current_language: string;
  enable_large_dataset_warning: boolean;
  warning_criteria: WarningCriteria;
  warn_on_exit_with_transforms: boolean;
  copy_tuple_sample_size: number;
  query_memory_limit_mb: number;
  warning_threshold_rows: number;
  warning_threshold_cells: number;
  warning_threshold_filesize_mb: number;
  bug_report_email: string;
  enable_crash_reporting: boolean;
  mcp_allow_clients: boolean;
  detect_column_types: boolean;
  show_query_panel: boolean;
  query_panel_collapsed: boolean;
  query_panel_height: number;
  show_steps_panel: boolean;
  show_ai_panel: boolean;
  date_order: "auto" | "day_first" | "month_first";
  [key: string]: unknown;
}

export interface Language {
  code: string;
  name: string;
  native_name: string;
  flag: string;
}

export interface ParamDef {
  name: string;
  logical_type: string;
  required: boolean;
  description: string;
}

/** One of Excel's functions in the reference. */
export interface ExcelEntry {
  name: string;
  /** math, statistics, conditional, logical, information, text, date, lookup, tables */
  category: string;
  /** The ParVu function to write. */
  function: string | null;
  /** For functions that don't fit a formula: what does the job. */
  instead: string | null;
}

export interface FunctionDef {
  name: string;
  params: ParamDef[];
  return_type: string;
  description: string;
  example: string;
  sql_name: string;
  sql_template?: string | null;
}

export interface ThemeInfo {
  name: string;
  css: string;
}

export interface ThemeDetails {
  name: string;
  description: string;
  author: string;
  builtin: boolean;
}

export interface Bootstrap {
  version: string;
  window_label: string;
  settings: Settings;
  language: string;
  languages: Language[];
  translations: Record<string, string>;
  english: Record<string, string>;
  theme: ThemeInfo;
  themes: string[];
  recents: string[];
  extensions: string[];
  cast_types: string[];
  parquet_compressions: string[];
  functions: FunctionDef[];
  /** Excel's functions and what to write for each in ParVu. */
  excel_functions?: ExcelEntry[];
  methods: Record<string, string[]>;
  crash_report: string | null;
  initial_files: string[];
  log_dir: string;
}

export interface ExpressionPreview {
  sql: string;
  kind: { type: "assignment"; column: string } | { type: "table_query" } | { type: "scalar" } | null;
}

export interface ApiError {
  kind: string;
  message: string;
  line?: number;
  column?: number;
}

let translate: ((key: string) => string) | null = null;

/** Lets errorMessage translate errors the backend reports by kind only. */
export function setErrorTranslator(fn: (key: string) => string): void {
  translate = fn;
}

/** Human-readable message of anything a command rejected with. */
export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && (e as ApiError).kind === "memory" && translate) return translate("error.memory_guard");
  if (e && typeof e === "object" && "message" in e) return String((e as ApiError).message);
  return String(e);
}

/** AI clients connected through `parvu mcp --attach` (AI → Connect an AI client). */
export interface McpStatus {
  enabled: boolean;
  listening: boolean;
  port: number | null;
  clients: number;
  /** Program and arguments clients run to connect. */
  command: string;
  args: string[];
  error: string | null;
}

/** A change an AI client asks for; the user approves or rejects it. */
export interface McpProposal {
  id: number;
  window: string;
  client: string;
  tab_id: number;
  table: string;
  /** "new_table": a table operation, which opens its result in a new tab. */
  kind: "operation" | "new_table" | "formula" | "undo";
  label: StepLabel;
  rationale: string | null;
  sql: string | null;
  rows_before: number;
  rows_after: number | null;
  columns: ColumnMeta[];
  rows: Cell[][];
}

// ---------- built-in AI assistant ----------

export type ProviderKind = "anthropic" | "open_ai_compatible";
export type Effort = "low" | "medium" | "high" | "xhigh" | "max";

/** A configured model provider (no key: keys live in the system keychain). */
export interface ProviderConfig {
  id: string;
  kind: ProviderKind;
  display_name: string;
  model: string;
  base_url: string | null;
  /** Environment variable with the key, used when the keychain has none. */
  key_env: string | null;
  max_output_tokens: number;
  effort: Effort;
  server_fallback: boolean;
  request_timeout_s: number;
}

export interface ProviderView {
  config: ProviderConfig;
  key_source: "keychain" | "environment" | "none";
  is_default: boolean;
}

export interface ProvidersInfo {
  providers: ProviderView[];
  /** Whether a system keychain is available for storing keys. */
  keychain: boolean;
}

export interface AiUsage {
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
}

export type AiStop =
  | { type: "end_turn" }
  | { type: "tool_use" }
  | { type: "max_tokens" }
  | { type: "refusal"; category: string | null }
  | { type: "other"; reason: string };

export type AiDelta =
  | { type: "text"; text: string }
  | { type: "thinking"; text: string }
  | { type: "tool_start"; id: string; name: string }
  | { type: "fallback"; from: string; to: string };

export type AgentEvent =
  | { type: "delta"; delta: AiDelta }
  | { type: "tool_call"; id: string; name: string; input: unknown }
  | { type: "tool_result"; id: string; preview: string; is_error: boolean }
  | { type: "usage"; usage: AiUsage; model: string; cost_usd: number | null }
  | { type: "done"; stop: AiStop };

export interface AiRunEvent {
  run_id: number;
  tab_id: number | null;
  event: AgentEvent;
}

export interface AiRunFinished {
  run_id: number;
  tab_id: number | null;
  error: string | null;
  cancelled: boolean;
}

/**
 * Invoke a backend command. "healed" means a CSV value deep in a file didn't
 * parse and the backend now reads that column as text, so the same call is
 * worth repeating (a few times: another column may follow).
 */
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  for (let attempt = 0; ; attempt++) {
    try {
      return await invoke<T>(cmd, args);
    } catch (e) {
      if (attempt < 8 && (e as { kind?: string } | null)?.kind === "healed") continue;
      throw e;
    }
  }
}

export const api = {
  sourceColumns: (id: number) => call<SourceColumn[]>("source_columns", { id }),
  setFormula: (id: number, name: string, source: string, after?: string | null) =>
    call<TabSummary>("set_formula", { id, name, source, after: after ?? null }),
  removeFormula: (id: number, name: string) => call<TabSummary>("remove_formula", { id, name }),
  previewConversion: (id: number, column: string, to: Reading, n?: number) =>
    call<{ before: Cell; after: Cell; failed: boolean }[]>("preview_conversion", { id, column, to, n }),
  previewColumnFn: (id: number, column: string, func: ColumnFn, values: Cell[]) =>
    call<[Cell, Cell][]>("preview_column_fn", { id, column, func, values }),
  jsonKeys: (id: number, column: string, full: boolean, options?: JobOptions) =>
    startJob((job) => call<JsonKey[]>("json_keys", { id, column, full, job }), options),
  suggestReading: (id: number, column: string) => call<Reading>("suggest_reading", { id, column }),
  conversionErrorCount: (id: number, column: string, to: Reading, full: boolean, options?: JobOptions) =>
    startJob(
      (job) => call<{ errors: number; checked: number; complete: boolean }>("conversion_error_count", { id, column, to, full, job }),
      options,
    ),
  setSourceReading: (id: number, column: string, reading: Reading) =>
    call<TabSummary>("set_source_reading", { id, column, reading }),
  bootstrap: () => call<Bootstrap>("bootstrap"),
  updateSettings: (values: Partial<Settings>) => call<Settings>("update_settings", { values }),
  resetSettings: () => call<Settings>("reset_settings"),
  setLanguage: (code: string) =>
    call<{ language: string; translations: Record<string, string> }>("set_language", { code }),
  setTheme: (name: string) => call<ThemeInfo>("set_theme", { name }),
  themeCss: (name: string) => call<string>("theme_css", { name }),
  listThemes: () => call<ThemeDetails[]>("list_themes"),
  importTheme: (path: string) => call<string>("import_theme", { path }),
  exportTheme: (name: string, path: string) => call<void>("export_theme", { name, path }),
  deleteTheme: (name: string) => call<void>("delete_theme", { name }),
  recents: () => call<string[]>("recents"),
  clearRecents: () => call<void>("clear_recents"),
  newWindow: () => call<string>("new_window"),

  openFile: (path: string, options?: CsvOptions, sheet?: string) =>
    call<TabSummary>("open_file", { path, options: options ?? null, sheet: sheet ?? null }),
  /** The sheets of the workbook a tab shows, with unsaved sheet changes. */
  excelSheets: (id: number) => call<SheetsView>("excel_sheets", { id }),
  sheetOp: (id: number, op: SheetOp) => call<SheetsView>("sheet_op", { id, op }),
  /** Show another sheet in the tab (by its stored name); the tab starts afresh. */
  setSheet: (id: number, sheet: string) => call<TabSummary>("set_sheet", { id, sheet }),
  previewCsv: (path: string, options: CsvOptions) => call<CsvPreview>("preview_csv", { path, options }),
  closeTab: (id: number) => call<void>("close_tab", { id }),
  tabs: () => call<TabSummary[]>("tabs"),
  tabSummary: (id: number) => call<TabSummary>("tab_summary", { id }),
  getPage: (id: number, page: number) => call<Page>("get_page", { id, page }),
  columns: (id: number) => call<ColumnMeta[]>("columns", { id }),

  runSql: (id: number, sql: string) => call<TabSummary>("run_sql", { id, sql }),
  catalog: () => call<[string, ColumnMeta[]][]>("catalog"),
  runExpression: (id: number, text: string) => call<TabSummary>("run_expression", { id, text }),
  previewExpression: (id: number, text: string) => call<ExpressionPreview>("preview_expression", { id, text }),
  applyOp: (id: number, op: Operation) => call<TabSummary>("apply_op", { id, op }),
  /** A table operation or query into a new tab, as a job. */
  makeTable: (id: number, derivation: Derivation, options?: JobOptions) =>
    startJob((job) => call<MadeTable>("make_table", { id, derivation, job }), options),
  /** Compute a result tab again from its source's current data. */
  refreshTable: (id: number, options?: JobOptions) => startJob((job) => call<MadeTable>("refresh_table", { id, job }), options),
  /** Write a result too big for memory to a file; returns rows written. */
  tableToFile: (id: number, derivation: Derivation, path: string, options?: JobOptions) =>
    startJob((job) => call<number>("table_to_file", { id, derivation, path, job }), options),
  addComputedColumn: (id: number, name: string, text: string) =>
    call<TabSummary>("add_computed_column", { id, name, text }),
  replaceValues: (id: number, request: ReplaceRequest) => call<TabSummary>("replace_values", { id, request }),
  undo: (id: number) => call<{ label: StepLabel; summary: TabSummary }>("undo", { id }),
  resetTab: (id: number) => call<TabSummary>("reset_tab", { id }),
  editCell: (id: number, row: number, column: string, value: Cell) =>
    call<TabSummary>("edit_cell", { id, row, column, value }),
  editCells: (id: number, cells: { row: number; column: string; value: Cell }[]) =>
    call<TabSummary>("edit_cells", { id, cells }),
  saveTab: (id: number, path: string | null) => call<TabSummary>("save_tab", { id, path }),

  /** Values to suggest for a filter's value box (most frequent in the first rows). */
  valueSuggestions: (id: number, column: string) => startJob((job) => call<string[]>("value_suggestions", { id, column, job })),
  columnStats: (id: number, column: string, options?: JobOptions) =>
    startJob((job) => call<ColumnStats>("column_stats", { id, column, job }), options),
  chartData: (id: number, spec: ChartSpec, options?: JobOptions) =>
    startJob((job) => call<ChartData>("chart_data", { id, spec, job }), options),
  saveChartImage: (path: string, bytes: Uint8Array) => call<void>("save_chart_image", { path, bytes: Array.from(bytes) }),
  /** Numeric min and max of each column, in one scan (colour scales). */
  columnRanges: (id: number, columns: string[], options?: JobOptions) =>
    startJob((job) => call<([number, number] | null)[]>("column_ranges", { id, columns, job }), options),
  /** Unique values, read piece by piece; new values arrive as progress `data`. */
  uniqueValues: (id: number, column: string, options?: JobOptions) =>
    startJob((job) => call<UniqueValues>("unique_values", { id, column, job }), options),
  columnTuple: (id: number, column: string, mode: "first" | "random", n: number, options?: JobOptions) =>
    startJob((job) => call<string>("column_tuple", { id, column, mode, n, job }), options),
  find: (id: number, search: SearchOptions, options?: JobOptions) =>
    startJob((job) => call<SearchHit | null>("find", { id, options: search, job }), options),

  startExport: (id: number, options: ExportOptions) => call<number>("start_export", { id, options }),
  cancelExport: (job: number) => call<void>("cancel_export", { job }),
  mcpStatus: () => call<McpStatus>("mcp_status"),
  mcpSetEnabled: (enabled: boolean) => call<McpStatus>("mcp_set_enabled", { enabled }),
  /** Answer an approval card; false when the request is gone. */
  mcpDecide: (id: number, approve: boolean) => call<boolean>("mcp_decide", { id, approve }),
  aiProviders: () => call<ProvidersInfo>("ai_providers"),
  /** `key`: undefined keeps the stored key, "" removes it. */
  aiSaveProvider: (config: ProviderConfig, key: string | undefined, makeDefault: boolean) =>
    call<ProvidersInfo>("ai_save_provider", { config, key: key ?? null, make_default: makeDefault }),
  aiDeleteProvider: (id: string) => call<ProvidersInfo>("ai_delete_provider", { id }),
  aiTestProvider: (id: string) => call<string>("ai_test_provider", { id }),
  aiAsk: (providerId: string, tabId: number | null, question: string) =>
    call<number>("ai_ask", { provider_id: providerId, tab_id: tabId, question }),
  aiCancel: (runId: number, tabId: number | null) => call<boolean>("ai_cancel", { run_id: runId, tab_id: tabId }),
  aiClearChat: (tabId: number | null) => call<void>("ai_clear_chat", { tab_id: tabId }),
};
