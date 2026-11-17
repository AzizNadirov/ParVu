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
}

// table[column], [column], ["column name"], ['column name']
const REF = /(?:\b[A-Za-z_][A-Za-z0-9_]*)?\[(?:"([^"]*)"|'([^']*)'|([A-Za-z_][A-Za-z0-9_]*))\]/g;

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
    out.push({ from: m.index ?? 0, to: (m.index ?? 0) + m[0].length, column });
  }
  return out;
}

/** A colour per referenced column, in order of first appearance. */
export function refColors(refs: FormulaRef[], dark = false): Map<string, string> {
  const palette = dark ? REF_COLORS_DARK : REF_COLORS_LIGHT;
  const colors = new Map<string, string>();
  for (const r of refs) if (!colors.has(r.column)) colors.set(r.column, palette[colors.size % palette.length]);
  return colors;
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
