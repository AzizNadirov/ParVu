// Text formats for copying grid selections (ports of DataTableView._copy_selection).
import type { Cell } from "./api";

export type CopyFormat = "tsv" | "csv" | "markdown";

const text = (c: Cell) => c ?? "";

function csvField(v: string): string {
  return /[",\n\r]/.test(v) ? `"${v.replace(/"/g, '""')}"` : v;
}

/** Rows (and optional header) as TSV, CSV or a Markdown table. */
export function formatRows(rows: Cell[][], format: CopyFormat, header?: string[]): string {
  const all: string[][] = [...(header ? [header] : []), ...rows.map((r) => r.map(text))];
  switch (format) {
    case "tsv":
      return all.map((r) => r.map((v) => v.replace(/[\t\n\r]/g, " ")).join("\t")).join("\n");
    case "csv":
      return all.map((r) => r.map(csvField).join(",")).join("\n");
    case "markdown":
      return markdownTable(header ?? all[0]?.map((_, i) => `col${i + 1}`) ?? [], header ? all.slice(1) : all);
  }
}

// ---------------------------------------------------------------- Markdown
//
// A GitHub-flavoured pipe table. File → Export writes the same text
// (`markdown_*` in crates/parvu-core/src/export.rs); both are tested against
// crates/parvu-core/tests/golden/markdown.json, so keep them in step.

/** Header of the row-number column. */
export const MARKDOWN_ROW_NUMBER_LABEL = "#";

export interface MarkdownOptions {
  /** Column names in the header line (default true). A pipe table can't do
   * without its header line, so without one the line is there but blank. */
  header?: boolean;
  /** A first column with 1-based row numbers, starting at `firstRow`. */
  rowNumbers?: boolean;
  /** Number of the first row (absolute, 1-based); default 1. */
  firstRow?: number;
  /** Right-align these columns (numbers). */
  numeric?: boolean[];
}

/** A cell made safe inside a pipe table: `|` becomes `\|` (backslashes right
 * before it are doubled, so they stay visible), line breaks become `<br>`. */
export function markdownCell(v: string): string {
  return v.replace(/(\\*)\|/g, (_, b: string) => `${b}${b}\\|`).replace(/\r\n|\r|\n/g, "<br>");
}

const mdLine = (cells: string[]) => `|${cells.map((c) => ` ${c} |`).join("")}`;

/** Rows as a Markdown table; NULL is an empty cell. */
export function markdownTable(columns: string[], rows: Cell[][], o: MarkdownOptions = {}): string {
  const header = o.header ?? true;
  const first = o.firstRow ?? 1;
  const num = (i: number) => !!o.numeric?.[i];
  const head = [...(o.rowNumbers ? [MARKDOWN_ROW_NUMBER_LABEL] : []), ...columns.map(markdownCell)].map((c) => (header ? c : ""));
  const delim = [...(o.rowNumbers ? ["---:"] : []), ...columns.map((_, i) => (num(i) ? "---:" : "---"))];
  const body = rows.map((r, i) => mdLine([...(o.rowNumbers ? [String(first + i)] : []), ...r.map((c) => markdownCell(text(c)))]));
  return [mdLine(head), mdLine(delim), ...body].join("\n");
}

export type MarkdownCopy = "table" | "row_numbers" | "no_header";

/**
 * A grid selection as a Markdown table. Grid rows are page-relative, so the
 * row numbers start at `offset + range.y + 1` (absolute, 1-based).
 */
export function selectionMarkdown(
  page: { offset: number; columns: { dtype: string }[] },
  range: { x: number; y: number; width: number },
  sel: { rows: Cell[][]; columns: string[] },
  mode: MarkdownCopy,
  isNumeric: (dtype: string) => boolean,
): { text: string; first: number; last: number } {
  const first = page.offset + range.y + 1;
  const numeric = page.columns.slice(range.x, range.x + range.width).map((c) => isNumeric(c.dtype));
  const text = markdownTable(sel.columns, sel.rows, {
    header: mode !== "no_header",
    rowNumbers: mode === "row_numbers",
    firstRow: first,
    numeric,
  });
  return { text, first, last: first + sel.rows.length - 1 };
}

/** Python-style tuple of the given cells, as "copy values as tuple" does for one page. */
export function pythonTuple(values: Cell[], numeric: boolean): string {
  const repr = (v: Cell) => {
    if (v === null) return "None";
    if (numeric && v.trim() !== "" && !Number.isNaN(Number(v))) return v;
    const quote = v.includes("'") && !v.includes('"') ? '"' : "'";
    const body = v.replace(/\\/g, "\\\\").replace(/\n/g, "\\n").replace(/\t/g, "\\t").replace(new RegExp(quote, "g"), `\\${quote}`);
    return `${quote}${body}${quote}`;
  };
  return `(${values.map(repr).join(", ")})`;
}
