// Typed wrappers around the Tauri commands in crates/parvu-app/src/commands.rs.
// Every call rejects with an ApiError.
import { invoke } from "@tauri-apps/api/core";

export type Cell = string | null;

export interface ColumnMeta {
  name: string;
  dtype: string;
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
}

export type Keep = "first" | "last";
export type JoinType = "inner" | "left" | "right" | "full_outer";

export type Operation =
  | { op: "sort"; column: string; ascending: boolean }
  | { op: "rename"; column: string; new_name: string }
  | { op: "remove"; column: string }
  | { op: "change_type"; column: string; new_type: string }
  | { op: "duplicate"; column: string }
  | { op: "drop_duplicates"; columns: string[]; keep: Keep }
  | { op: "drop_null"; column: string; null_value: string | null }
  | { op: "filter_values"; column: string; values: Cell[] }
  | { op: "join"; right: string; join_type: JoinType; left_key: string; right_key: string }
  | { op: "append"; tables: string[]; distinct: boolean }
  | { op: "add_column"; name: string; expression: string };

export interface ReplaceRequest {
  column: string;
  pattern: string;
  replacement: string;
  output_column: string;
  case_sensitive: boolean;
  regex: boolean;
}

export interface ColumnStats {
  column: string;
  dtype: string;
  row_count: number;
  non_null: number;
  null: number;
  distinct: number;
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
  warning_threshold_rows: number;
  warning_threshold_cells: number;
  warning_threshold_filesize_mb: number;
  bug_report_email: string;
  enable_crash_reporting: boolean;
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

/** Human-readable message of anything a command rejected with. */
export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return String((e as ApiError).message);
  return String(e);
}

const call = <T>(cmd: string, args?: Record<string, unknown>) => invoke<T>(cmd, args);

export const api = {
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

  openFile: (path: string) => call<TabSummary>("open_file", { path }),
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
  addComputedColumn: (id: number, name: string, text: string) =>
    call<TabSummary>("add_computed_column", { id, name, text }),
  replaceValues: (id: number, request: ReplaceRequest) => call<TabSummary>("replace_values", { id, request }),
  undo: (id: number) => call<{ label: StepLabel; summary: TabSummary }>("undo", { id }),
  resetTab: (id: number) => call<TabSummary>("reset_tab", { id }),
  editCell: (id: number, row: number, column: string, value: Cell) =>
    call<TabSummary>("edit_cell", { id, row, column, value }),
  saveTab: (id: number, path: string | null) => call<TabSummary>("save_tab", { id, path }),

  columnStats: (id: number, column: string) => call<ColumnStats>("column_stats", { id, column }),
  uniqueValues: (id: number, column: string) => call<Cell[]>("unique_values", { id, column }),
  columnTuple: (id: number, column: string, mode: "first" | "random", n: number) =>
    call<string>("column_tuple", { id, column, mode, n }),
  find: (id: number, options: SearchOptions) => call<SearchHit | null>("find", { id, options }),

  startExport: (id: number, options: ExportOptions) => call<number>("start_export", { id, options }),
  cancelExport: (job: number) => call<void>("cancel_export", { job }),
};
