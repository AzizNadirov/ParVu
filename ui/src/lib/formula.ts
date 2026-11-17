// Column formulas in the formula bar: the column references in a formula,
// their colours (the same in the editor and in the grid), and inserting a
// reference.

/** Colours of referenced columns, in order of first appearance; the
 * categorical chart palette (light / dark), checked for colour-blind
 * readers. */
export const REF_COLORS_LIGHT = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"];
export const REF_COLORS_DARK = ["#3987e5", "#d95926", "#199e70", "#c98500", "#d55181", "#0ca30c", "#9085e9", "#e66767"];

export interface FormulaRef {
  /** Offsets of the whole reference (`[qty]` or `sales[qty]`) in the text. */
  from: number;
  to: number;
  column: string;
  /** Last column of a range `[q1:q4]` (then `column` is the first). */
  toColumn?: string;
}

const NAME = `(?:"([^"]*)"|'([^']*)'|([A-Za-z_][A-Za-z0-9_]*))`;
// table[column], [column], ["column name"], ['column name'], [first:last]
const REF = new RegExp(`(?:\\b[A-Za-z_][A-Za-z0-9_]*)?\\[${NAME}(?::${NAME})?\\]`, "g");

/** The column references in a formula, outside string literals. */
export function formulaRefs(text: string): FormulaRef[] {
  const out: FormulaRef[] = [];
  // Blank out string literals that are not inside brackets, so a bracket
  // in 'some [text]' isn't taken for a reference.
  let masked = "";
  let quote: string | null = null;
  let depth = 0;
  for (const ch of text) {
    if (quote) {
      masked += ch === quote ? ch : depth > 0 ? ch : " ";
      if (ch === quote) quote = null;
      continue;
    }
    if ((ch === "'" || ch === '"') && depth === 0) quote = ch;
    if (ch === "[") depth++;
    if (ch === "]") depth = Math.max(0, depth - 1);
    if ((ch === "'" || ch === '"') && depth > 0) quote = ch;
    masked += ch;
  }
  for (const m of masked.matchAll(REF)) {
    const column = m[1] ?? m[2] ?? m[3] ?? "";
    const toColumn = m[4] ?? m[5] ?? m[6];
    const from = m.index ?? 0;
    out.push(toColumn !== undefined ? { from, to: from + m[0].length, column, toColumn } : { from, to: from + m[0].length, column });
  }
  return out;
}

/** What a reference names: a column, or a range `first:last`. */
export const refKey = (r: FormulaRef) => (r.toColumn !== undefined ? `${r.column}:${r.toColumn}` : r.column);

/** A colour per reference (column or range), in order of first appearance. */
export function refColors(refs: FormulaRef[], dark = false): Map<string, string> {
  const palette = dark ? REF_COLORS_DARK : REF_COLORS_LIGHT;
  const colors = new Map<string, string>();
  for (const r of refs) {
    const k = refKey(r);
    if (!colors.has(k)) colors.set(k, palette[colors.size % palette.length]);
  }
  return colors;
}

/** The grid tint of each referenced column; a range tints every column in it. */
export function columnHighlights(refs: FormulaRef[], columns: string[], dark = false): Map<string, string> {
  const colors = refColors(refs, dark);
  const out = new Map<string, string>();
  for (const r of refs) {
    const color = colors.get(refKey(r))!;
    let names = [r.column];
    if (r.toColumn !== undefined) {
      const a = columns.indexOf(r.column);
      const b = columns.indexOf(r.toColumn);
      names = a < 0 || b < 0 ? [r.column, r.toColumn] : columns.slice(Math.min(a, b), Math.max(a, b) + 1);
    }
    for (const n of names) if (!out.has(n)) out.set(n, color);
  }
  return out;
}

const plainName = (column: string) => (/^[A-Za-z_][A-Za-z0-9_]*$/.test(column) ? column : `"${column.replace(/"/g, "")}"`);

/** The range reference `[first:last]`. */
export const rangeFor = (first: string, last: string) => `[${plainName(first)}:${plainName(last)}]`;

/**
 * Shift+click on a header while writing: the reference just before the
 * cursor becomes a range up to `column`. Returns the text change, or null
 * when there is no single-column reference right before the cursor.
 */
export function extendToRange(text: string, cursor: number, column: string): { from: number; to: number; insert: string } | null {
  const r = formulaRefs(text).find((x) => x.to === cursor && x.toColumn === undefined);
  return r ? { from: r.from, to: r.to, insert: rangeFor(r.column, column) } : null;
}

/**
 * Where a compiler error points, as an offset into the bar's text. The
 * compiler reports a 1-based line and column of the formula without its
 * leading "=" and spaces.
 */
export function errorOffset(text: string, line: number, column: number): number {
  const rest = text.trimStart().replace(/^=\s*/, "");
  let at = text.length - rest.length;
  const lines = rest.split("\n");
  for (let i = 0; i < line - 1 && i < lines.length; i++) at += lines[i].length + 1;
  return Math.min(text.length, at + Math.max(0, column - 1));
}

/** The reference to insert for a column: `[qty]`, or `["unit price"]`. */
export function refFor(column: string): string {
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(column) ? `[${column}]` : `["${column.replace(/"/g, "")}"]`;
}

/** A name for a new formula column not among `taken`: column1, column2… */
export function newFormulaName(taken: string[], base = "column"): string {
  for (let i = 1; ; i++) if (!taken.includes(`${base}${i}`)) return `${base}${i}`;
}

/** The formula without a leading "=". */
export const formulaBody = (text: string) => text.trim().replace(/^=\s*/, "");
