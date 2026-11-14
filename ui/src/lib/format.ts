// Conditional formatting: rules that colour cells of the grid (Calc's
// conditional formatting). Rules belong to the window, not the data, so they
// are not steps and never change the file.

export const FORMAT_COLORS = ["primary", "secondary", "warning", "error"] as const;
export type FormatColor = (typeof FORMAT_COLORS)[number];

export const HIGHLIGHT_OPS = ["gt", "ge", "lt", "le", "eq", "ne", "between", "contains", "is_null"] as const;
export type HighlightOp = (typeof HIGHLIGHT_OPS)[number];

export type FormatRule =
  /** Background from light to strong by the value's place between the column's min and max. */
  | { kind: "scale"; column: string; color: FormatColor }
  | { kind: "highlight"; column: string; op: HighlightOp; value: string; value2: string; color: FormatColor };

/** Numeric value of a cell's text, or null. */
function num(v: string | null): number | null {
  if (v === null || v.trim() === "") return null;
  const n = Number(v);
  return Number.isFinite(n) ? n : null;
}

/**
 * Compare two cell texts: as numbers when both are numbers, otherwise as text
 * (ISO dates and timestamps compare correctly as text).
 */
export function compareCell(a: string, b: string): number {
  const x = num(a);
  const y = num(b);
  if (x !== null && y !== null) return x - y;
  return a < b ? -1 : a > b ? 1 : 0;
}

export function matchesHighlight(rule: Extract<FormatRule, { kind: "highlight" }>, v: string | null): boolean {
  if (rule.op === "is_null") return v === null;
  if (v === null) return false;
  const c = () => compareCell(v, rule.value);
  switch (rule.op) {
    case "gt":
      return c() > 0;
    case "ge":
      return c() >= 0;
    case "lt":
      return c() < 0;
    case "le":
      return c() <= 0;
    case "eq":
      return c() === 0;
    case "ne":
      return c() !== 0;
    case "between":
      return compareCell(v, rule.value) >= 0 && compareCell(v, rule.value2) <= 0;
    case "contains":
      return v.toLowerCase().includes(rule.value.toLowerCase());
  }
}

/** Position of `v` between min and max, 0..1, or null when it cannot be placed. */
export function scalePosition(v: string | null, range: { min: number; max: number } | undefined): number | null {
  const n = num(v);
  if (n === null || !range) return null;
  if (range.max === range.min) return 1;
  return Math.min(1, Math.max(0, (n - range.min) / (range.max - range.min)));
}

/** `color` (#rgb, #rrggbb or rgb()/rgba()) with the given alpha, as rgba(). */
export function withAlpha(color: string, alpha: number): string {
  const c = color.trim();
  let r = 0;
  let g = 0;
  let b = 0;
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(c);
  if (hex) {
    const h = hex[1].length === 3 ? [...hex[1]].map((x) => x + x).join("") : hex[1];
    r = parseInt(h.slice(0, 2), 16);
    g = parseInt(h.slice(2, 4), 16);
    b = parseInt(h.slice(4, 6), 16);
  } else {
    const m = /rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)/i.exec(c);
    if (!m) return c;
    [r, g, b] = [m[1], m[2], m[3]].map(Number);
  }
  return `rgba(${r}, ${g}, ${b}, ${Math.round(alpha * 100) / 100})`;
}

/**
 * Background for a cell under `rules` (only the rules of its column), or
 * null. Later rules win, so a highlight added after a scale shows on top.
 */
export function cellBackground(
  rules: FormatRule[],
  v: string | null,
  ranges: Record<string, { min: number; max: number }>,
  colors: Record<FormatColor, string>,
): string | null {
  let bg: string | null = null;
  for (const rule of rules) {
    if (rule.kind === "scale") {
      const p = scalePosition(v, ranges[rule.column]);
      if (p !== null) bg = withAlpha(colors[rule.color], 0.08 + p * 0.55);
    } else if (matchesHighlight(rule, v)) {
      bg = withAlpha(colors[rule.color], 0.35);
    }
  }
  return bg;
}
