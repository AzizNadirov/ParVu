// Pure helpers for charts: palette, axis ticks, number formats, the
// suggested chart for a column, and the chart title.
import type { ChartData, ChartKind, ChartSpec, ColumnMeta } from "./api";
import type { Translate } from "./i18n";

// Categorical palette, validated (scripts/validate_palette.js of the dataviz
// guide) against ParVu's chart surfaces: light on #ffffff, dark on #1d1b20.
// Slots are used in this fixed order, never cycled; past 8 the data folds
// into "Other", which is gray. In light mode three slots are below 3:1
// against white, so charts always offer a legend and a table view.
export const PALETTE_LIGHT = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"];
export const PALETTE_DARK = ["#3987e5", "#d95926", "#199e70", "#c98500", "#d55181", "#0ca30c", "#9085e9", "#e66767"];
export const OTHER_LIGHT = "#9b9a94";
export const OTHER_DARK = "#75746e";

/** Colour of each series: palette slots in order, gray for "Other". */
export function seriesColors(series: { other: boolean }[], dark: boolean): string[] {
  const pal = dark ? PALETTE_DARK : PALETTE_LIGHT;
  let slot = 0;
  return series.map((s) => (s.other ? (dark ? OTHER_DARK : OTHER_LIGHT) : pal[slot++ % pal.length]));
}

/** Relative luminance of "#rrggbb" or "rgb(r, g, b)"; 1 when unreadable. */
export function luminance(color: string): number {
  let r: number, g: number, b: number;
  const hex = color.trim().match(/^#([0-9a-f]{6})$/i);
  const rgb = color.match(/rgba?\(\s*(\d+)[,\s]+(\d+)[,\s]+(\d+)/i);
  if (hex) {
    const n = parseInt(hex[1], 16);
    [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  } else if (rgb) {
    [r, g, b] = [Number(rgb[1]), Number(rgb[2]), Number(rgb[3])];
  } else return 1;
  const lin = (c: number) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

export const isDark = (surface: string) => luminance(surface) < 0.2;

/** About `count` round tick values covering [min, max] (1, 2, 5 × 10ⁿ steps). */
export function niceTicks(min: number, max: number, count = 5): number[] {
  if (!Number.isFinite(min) || !Number.isFinite(max)) return [];
  if (min === max) {
    const pad = Math.abs(min) || 1;
    return niceTicks(min - pad, max + pad, count);
  }
  const span = max - min;
  const raw = span / Math.max(1, count);
  const mag = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? 10 * mag;
  const start = Math.floor(min / step) * step;
  const ticks: number[] = [];
  for (let v = start; v <= max + step * 1e-9; v += step) ticks.push(Number(v.toPrecision(12)));
  if (ticks[ticks.length - 1] < max) ticks.push(Number((ticks[ticks.length - 1] + step).toPrecision(12)));
  return ticks;
}

const group = (s: string) => s.replace(/\B(?=(\d{3})+(?!\d))/g, ",");

function fixed(v: number, decimals: number): string {
  const s = Number(v.toFixed(decimals)).toString();
  if (s.includes("e")) return s;
  const [int, frac] = s.split(".");
  const sign = int.startsWith("-") ? "-" : "";
  const body = group(int.replace("-", ""));
  return frac ? `${sign}${body}.${frac}` : `${sign}${body}`;
}

/**
 * Formatter for one axis: numbers in full under 10,000 (1,000 / 2,000),
 * compact on an axis that reaches 10,000 (10K / 1.5M), one style per axis.
 */
export function axisFormatter(ticks: number[]): (v: number) => string {
  const maxAbs = Math.max(0, ...ticks.map(Math.abs));
  const step = ticks.length > 1 ? Math.abs(ticks[1] - ticks[0]) : 1;
  if (maxAbs < 10_000) {
    const decimals = Math.min(6, decimalsOf(step));
    return (v) => fixed(v, decimals);
  }
  const [div, suffix] = maxAbs >= 1e12 ? [1e12, "T"] : maxAbs >= 1e9 ? [1e9, "B"] : maxAbs >= 1e6 ? [1e6, "M"] : [1e3, "K"];
  const decimals = Math.min(3, decimalsOf(step / div));
  return (v) => (v === 0 ? "0" : `${fixed(v / div, decimals)}${suffix}`);
}

/** Decimal places needed to write `v` exactly (0.25 → 2, 500 → 0). */
function decimalsOf(v: number): number {
  const s = String(Number(Math.abs(v).toPrecision(6)));
  if (s.includes("e-")) return Number(s.split("e-")[1]);
  return (s.split(".")[1] ?? "").length;
}

/** A value in a tooltip or label: grouped, up to 4 significant decimals. */
export function formatValue(v: number | null | undefined): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return "—";
  const a = Math.abs(v);
  const decimals = a >= 1000 ? 0 : a >= 100 ? 1 : a >= 1 ? 2 : 4;
  return fixed(v, decimals);
}

/** Compact value for labels at bar tips: 1,284 / 12.9K / 4.2M. */
export function compactValue(v: number | null | undefined): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return "";
  const a = Math.abs(v);
  if (a < 10_000) return formatValue(v);
  const [div, suffix] = a >= 1e12 ? [1e12, "T"] : a >= 1e9 ? [1e9, "B"] : a >= 1e6 ? [1e6, "M"] : [1e3, "K"];
  return `${fixed(v / div, 1)}${suffix}`;
}

/** Label shortened to `max` characters with an ellipsis. */
export function shorten(label: string, max: number): string {
  return label.length > max ? `${label.slice(0, Math.max(1, max - 1))}…` : label;
}

/** Kept for callers of the old helper name. */
export const formatTick = (v: number) => axisFormatter([0, v])(v);

// ---------- suggestions ----------

const isTimeType = (dtype: string) => /^(DATE|TIMESTAMP)/i.test(dtype);
const isNumericDtype = (dtype: string) =>
  /^(TINYINT|SMALLINT|INTEGER|INT|BIGINT|HUGEINT|UTINYINT|USMALLINT|UINTEGER|UBIGINT|UHUGEINT|FLOAT|REAL|DOUBLE|DECIMAL|NUMERIC)/i.test(dtype);
/** Looks like an identifier, not a measure: id, user_id, ID, code… */
const isIdLike = (name: string) => /(^|[_\s-])(id|ids|uuid|key|code|no|number)$/i.test(name) || /^id/i.test(name);
const isText = (dtype: string) => !isTimeType(dtype) && !isNumericDtype(dtype);

// Names of measures worth summing, best first; a price is a last resort
// (summing prices rarely means anything).
const MEASURE_NAMES = [/revenue|sales|turnover|income/i, /amount|total|profit|spend|cost/i, /value/i, /price/i];

export function columnRole(c: ColumnMeta): "time" | "measure" | "category" {
  if (isTimeType(c.dtype)) return "time";
  if (isNumericDtype(c.dtype) && !isIdLike(c.name)) return "measure";
  return "category";
}

/**
 * The chart that most likely answers "show me this column":
 * a date → values over time; a number → its distribution; text → how often
 * each value occurs. With no selection: a measure over time, else a measure
 * by category, else the first column's counts.
 */
export function suggestChart(columns: ColumnMeta[], selected?: string): Partial<ChartSpec> & { kind: ChartKind; x: string } {
  // Prefer a column that reads like an amount (revenue, sales, total…)
  // over the first number (often a count such as qty).
  const measures = columns.filter((c) => columnRole(c) === "measure" && c.name !== selected);
  const named = MEASURE_NAMES.map((re) => measures.find((c) => re.test(c.name))).find(Boolean);
  const measure = named ?? measures[0];
  const time = columns.find((c) => columnRole(c) === "time");
  const category = columns.find((c) => columnRole(c) === "category" && isText(c.dtype));
  const sel = columns.find((c) => c.name === selected);
  if (sel) {
    const role = columnRole(sel);
    if (role === "time") return { kind: "line", x: sel.name, y: measure?.name ?? null, agg: "sum" };
    if (role === "measure") return { kind: "histogram", x: sel.name, y: null };
    return { kind: "bar", x: sel.name, y: null, agg: "rows" };
  }
  if (time && measure) return { kind: "line", x: time.name, y: measure.name, agg: "sum" };
  if (category && measure) return { kind: "bar", x: category.name, y: measure.name, agg: "sum" };
  if (category) return { kind: "bar", x: category.name, y: null, agg: "rows" };
  if (measure) return { kind: "histogram", x: measure.name, y: null };
  return { kind: "bar", x: columns[0]?.name ?? "", y: null, agg: "rows" };
}

/**
 * The spec after switching to another kind of chart, with axes that suit it:
 * a scatter or histogram needs numbers on x, a bar or line chart reads best
 * grouped by a date or a category. Columns that already fit are kept.
 */
export function switchKind(spec: ChartSpec, kind: ChartKind, columns: ColumnMeta[]): ChartSpec {
  const role = (name: string | null | undefined) => {
    const c = columns.find((c) => c.name === name);
    return c ? columnRole(c) : null;
  };
  const isNum = (name: string | null | undefined) => !!columns.find((c) => c.name === name && isNumericDtype(c.dtype));
  const measures = columns.filter((c) => columnRole(c) === "measure").map((c) => c.name);
  const best = (skip: (string | null)[]) => {
    const left = measures.filter((m) => !skip.includes(m));
    return MEASURE_NAMES.map((re) => left.find((m) => re.test(m))).find(Boolean) ?? left[0] ?? null;
  };
  const next: ChartSpec = { ...spec, kind };
  if (kind === "histogram") {
    next.x = isNum(spec.x) && role(spec.x) === "measure" ? spec.x : isNum(spec.y) ? spec.y! : (best([]) ?? spec.x);
    return next;
  }
  if (kind === "scatter") {
    // A numeric x that is a measure stays (an id on x is rarely meant).
    const x = role(spec.x) === "measure" ? spec.x : (best([spec.y]) ?? spec.x);
    const y = isNum(spec.y) && spec.y !== x ? spec.y : best([x]);
    return { ...next, x, y };
  }
  // Bar, line, area: group by a date or category, not by a measure.
  if (role(spec.x) === "measure") {
    const time = columns.find((c) => columnRole(c) === "time")?.name;
    const category = columns.find((c) => columnRole(c) === "category" && isText(c.dtype))?.name;
    const x = (kind === "bar" ? (category ?? time) : (time ?? category)) ?? spec.x;
    const y = isNum(spec.y) ? spec.y : spec.x;
    return { ...next, x, y, agg: spec.agg === "rows" && y ? "sum" : spec.agg };
  }
  return next;
}

/** The chart's default title, e.g. "Sum of qty by region, split by product". */
export function chartTitle(spec: ChartSpec, t: Translate): string {
  if (spec.kind === "histogram") return t("chart.title.histogram", { x: spec.x });
  if (spec.kind === "scatter") {
    const base = t("chart.title.scatter", { x: spec.x, y: spec.y ?? "" });
    return spec.series ? `${base}${t("chart.title.split", { series: spec.series })}` : base;
  }
  const measure = spec.y ? t("chart.title.measure", { agg: t(`agg.${spec.agg}`), y: spec.y }) : t("chart.title.rows");
  const base = t("chart.title.by", { measure, x: spec.x });
  return spec.series ? `${base}${t("chart.title.split", { series: spec.series })}` : base;
}

/** Largest stacked total and smallest value, for the value axis. */
export function valueExtent(data: ChartData, stacked: boolean): [number, number] {
  let lo = 0;
  let hi = 0;
  const n = data.labels.length;
  if (stacked) {
    for (let i = 0; i < n; i++) {
      let pos = 0;
      let neg = 0;
      for (const s of data.series) {
        const v = s.values[i];
        if (v === null || v === undefined || !Number.isFinite(v)) continue;
        if (v >= 0) pos += v;
        else neg += v;
      }
      hi = Math.max(hi, pos);
      lo = Math.min(lo, neg);
    }
  } else {
    for (const s of data.series)
      for (const v of s.values) {
        if (v === null || !Number.isFinite(v)) continue;
        hi = Math.max(hi, v);
        lo = Math.min(lo, v);
      }
  }
  return [lo, hi];
}
