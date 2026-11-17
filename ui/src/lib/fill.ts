// Filling cells, as in a spreadsheet: continue a series from seed cells
// (the fill handle) and copy an edge row or column over a selection (fill
// down / up / left / right). Pure functions; the grid and App only apply the
// result as cell edits.
import type { Cell } from "./api";

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** One cell to write: page row, column index, new value. */
export interface FillCell {
  row: number;
  col: number;
  value: Cell;
}

export type FillDirection = "down" | "up" | "right" | "left";

// ---------------------------------------------------------------------------
// Series rules

const NUMBER = /^[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?$/;
// 2024-01-31, 2024-01-31 12:30:00, 2024-01-31T12:30:00.250
const DATE = /^(\d{4})-(\d{2})-(\d{2})(?:([ T])(\d{2}):(\d{2})(?::(\d{2})(?:\.(\d{1,9}))?)?)?$/;
// Text that ends in a whole number: "Item 1", "Q3", "INV-007"
const TEXT_NUMBER = /^(.*?)(\d+)$/s;

const DAY_MS = 86_400_000;

/** Month and day names; each list in order, matched without case. */
const NAME_LISTS: string[][] = [
  // English
  ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december"],
  ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"],
  ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"],
  ["mon", "tue", "wed", "thu", "fri", "sat", "sun"],
  // Russian
  ["январь", "февраль", "март", "апрель", "май", "июнь", "июль", "август", "сентябрь", "октябрь", "ноябрь", "декабрь"],
  ["янв", "фев", "мар", "апр", "май", "июн", "июл", "авг", "сен", "окт", "ноя", "дек"],
  ["понедельник", "вторник", "среда", "четверг", "пятница", "суббота", "воскресенье"],
  ["пн", "вт", "ср", "чт", "пт", "сб", "вс"],
  // Azerbaijani
  ["yanvar", "fevral", "mart", "aprel", "may", "iyun", "iyul", "avqust", "sentyabr", "oktyabr", "noyabr", "dekabr"],
  ["bazar ertəsi", "çərşənbə axşamı", "çərşənbə", "cümə axşamı", "cümə", "şənbə", "bazar"],
];

const decimals = (s: string) => {
  if (/e/i.test(s)) return -1;
  const dot = s.indexOf(".");
  return dot < 0 ? 0 : s.length - dot - 1;
};

/** A number without float noise (0.1 + 0.2 → "0.3"), with `places` decimals when known. */
function formatNumber(v: number, places: number): string {
  if (places > 0) return v.toFixed(places);
  const clean = Number.parseFloat(v.toPrecision(12));
  return Object.is(clean, -0) ? "0" : String(clean);
}

/** The common step between consecutive values, or null if they don't share one. */
function constantStep(values: number[], tolerance = 1e-9): number | null {
  const step = values[1] - values[0];
  for (let i = 2; i < values.length; i++) {
    if (Math.abs(values[i] - values[i - 1] - step) > tolerance * Math.max(1, Math.abs(step))) return null;
  }
  return step;
}

function repeat(seeds: Cell[], count: number): Cell[] {
  return Array.from({ length: count }, (_, i) => seeds[i % seeds.length]);
}

function numberSeries(seeds: string[], count: number): Cell[] | null {
  if (!seeds.every((s) => NUMBER.test(s.trim()))) return null;
  // One number is copied, as in a spreadsheet.
  if (seeds.length === 1) return repeat(seeds, count);
  const values = seeds.map((s) => Number(s.trim()));
  // A straight line through the first and last seed.
  const step = (values[values.length - 1] - values[0]) / (values.length - 1);
  const places = seeds.map((s) => decimals(s.trim()));
  // Fixed decimals only when every seed has the same number of them (1.50, 1.75 → 2.00).
  const fixed = places.every((p) => p === places[0] && p > 0) ? places[0] : 0;
  const last = values[values.length - 1];
  return Array.from({ length: count }, (_, i) => formatNumber(last + step * (i + 1), fixed));
}

interface ParsedDate {
  ms: number;
  y: number;
  m: number;
  d: number;
  /** Milliseconds since midnight. */
  time: number;
  hasTime: boolean;
  sep: string;
  seconds: boolean;
  fraction: number;
}

function parseDate(s: string): ParsedDate | null {
  const m = DATE.exec(s.trim());
  if (!m) return null;
  const [y, mo, d] = [Number(m[1]), Number(m[2]) - 1, Number(m[3])];
  const ms = Date.UTC(y, mo, d);
  const check = new Date(ms);
  if (check.getUTCMonth() !== mo || check.getUTCDate() !== d) return null;
  const hasTime = m[4] !== undefined;
  const frac = m[8] ?? "";
  const time = hasTime
    ? ((Number(m[5]) * 60 + Number(m[6])) * 60 + Number(m[7] ?? 0)) * 1000 + Math.round(Number(`0.${frac || "0"}`) * 1000)
    : 0;
  return { ms: ms + time, y, m: mo, d, time, hasTime, sep: m[4] ?? " ", seconds: m[7] !== undefined, fraction: frac.length };
}

const pad = (n: number, w = 2) => String(n).padStart(w, "0");

function formatDate(ms: number, like: ParsedDate): string {
  const t = new Date(ms);
  const date = `${pad(t.getUTCFullYear(), 4)}-${pad(t.getUTCMonth() + 1)}-${pad(t.getUTCDate())}`;
  if (!like.hasTime) return date;
  let time = `${pad(t.getUTCHours())}:${pad(t.getUTCMinutes())}`;
  if (like.seconds) time += `:${pad(t.getUTCSeconds())}`;
  if (like.fraction) time += "." + pad(t.getUTCMilliseconds(), 3).padEnd(like.fraction, "0").slice(0, like.fraction);
  return `${date}${like.sep}${time}`;
}

function monthLength(y: number, m: number): number {
  const d = new Date(Date.UTC(2000, m + 1, 0));
  d.setUTCFullYear(y, m + 1, 0);
  return d.getUTCDate();
}

/** `months` months after a date; the day is clamped to the month's length (31 Jan + 1 month → 28/29 Feb). */
function addMonths(p: ParsedDate, months: number): number {
  const total = p.y * 12 + p.m + months;
  const y = Math.floor(total / 12);
  const m = total - y * 12;
  const days = monthLength(y, m);
  const date = new Date(Date.UTC(0, m, Math.min(p.d, days)));
  date.setUTCFullYear(y);
  return date.getTime() + p.time;
}

function dateSeries(seeds: string[], count: number): Cell[] | null {
  const dates = seeds.map(parseDate);
  if (dates.some((d) => d === null)) return null;
  const ds = dates as ParsedDate[];
  const last = ds[ds.length - 1];
  // One date continues by a day.
  if (ds.length === 1) return Array.from({ length: count }, (_, i) => formatDate(last.ms + DAY_MS * (i + 1), last));
  // Same day of the month (or all month ends) and time: step by whole
  // months or years.
  const sameTime = ds.every((d) => d.time === ds[0].time);
  const sameDay = ds.every((d) => d.d === ds[0].d);
  const monthEnds = ds.every((d) => d.d === monthLength(d.y, d.m));
  if (sameTime && (sameDay || monthEnds)) {
    const months = constantStep(ds.map((d) => d.y * 12 + d.m));
    if (months !== null && months !== 0) {
      // Month ends stay month ends: 31 Jan, 29 Feb → 31 Mar.
      const from = monthEnds && !sameDay ? { ...last, d: 31 } : last;
      return Array.from({ length: count }, (_, i) => formatDate(addMonths(from, months * (i + 1)), last));
    }
  }
  const step = constantStep(ds.map((d) => d.ms), 0);
  if (step === null) return null;
  return Array.from({ length: count }, (_, i) => formatDate(last.ms + step * (i + 1), last));
}

/** Upper, Title or lower case, taken from a sample. */
function caseLike(sample: string, word: string): string {
  if (sample === sample.toLocaleUpperCase() && sample !== sample.toLocaleLowerCase()) return word.toLocaleUpperCase();
  const first = sample.charAt(0);
  if (first !== first.toLocaleLowerCase()) return word.charAt(0).toLocaleUpperCase() + word.slice(1);
  return word;
}

function nameSeries(seeds: string[], count: number): Cell[] | null {
  const keys = seeds.map((s) => s.trim().toLocaleLowerCase());
  for (const list of NAME_LISTS) {
    const idx = keys.map((k) => list.indexOf(k));
    if (idx.some((i) => i < 0)) continue;
    const n = list.length;
    let step = 1;
    if (idx.length > 1) {
      // Steps are counted forward around the list (Dec → Jan is +1).
      const diffs = idx.slice(1).map((v, i) => (((v - idx[i]) % n) + n) % n);
      if (!diffs.every((d) => d === diffs[0])) return null;
      step = diffs[0];
      // A backward series (Mar, Feb) keeps going backward.
      if (step > n / 2) step -= n;
    }
    const last = idx[idx.length - 1];
    const sample = seeds[seeds.length - 1].trim();
    return Array.from({ length: count }, (_, i) => caseLike(sample, list[(((last + step * (i + 1)) % n) + n) % n]));
  }
  return null;
}

function textNumberSeries(seeds: string[], count: number): Cell[] | null {
  const parts = seeds.map((s) => TEXT_NUMBER.exec(s));
  if (parts.some((p) => p === null)) return null;
  const ps = parts as RegExpExecArray[];
  const prefix = ps[0][1];
  if (!ps.every((p) => p[1] === prefix)) return null;
  const nums = ps.map((p) => Number(p[2]));
  const step = nums.length === 1 ? 1 : constantStep(nums);
  if (step === null) return null;
  // Keep leading zeros: INV-007 → INV-008.
  const width = ps[ps.length - 1][2].startsWith("0") ? ps[ps.length - 1][2].length : 0;
  const last = nums[nums.length - 1];
  // Below zero there is no number to write after the text: repeat instead.
  if (last + step * count < 0) return null;
  return Array.from({ length: count }, (_, i) => prefix + String(last + step * (i + 1)).padStart(width, "0"));
}

/**
 * The next `count` values after `seeds`, nearest first.
 *
 * Numbers continue the step between the seeds (1, 2 → 3, 4; one number is
 * copied); dates and timestamps continue by day, or by month or year when
 * the seeds share a day of the month; month and day names (English,
 * Russian, Azerbaijani) and text ending in a number (Item 1 → Item 2)
 * continue by their step, or by one. Anything else repeats the seeds.
 *
 * To fill backward (up or left), pass the seeds farthest first.
 */
export function continueSeries(seeds: Cell[], count: number): Cell[] {
  if (count <= 0 || seeds.length === 0) return [];
  if (seeds.some((s) => s === null || s.trim() === "")) return repeat(seeds, count);
  const text = seeds as string[];
  // Whole numbers with leading zeros are codes (007 → 008), not amounts.
  const codes = text.some((s) => /^0\d/.test(s.trim()));
  return (
    (codes ? null : numberSeries(text, count)) ??
    dateSeries(text, count) ??
    nameSeries(text, count) ??
    textNumberSeries(text, count) ??
    repeat(seeds, count)
  );
}

// ---------------------------------------------------------------------------
// Grid fills

type Get = (row: number, col: number) => Cell;

/**
 * The cells a fill-handle drag writes: `source` is the selection the drag
 * started from, `dest` the whole selection after it (source included). Each
 * column (or row, for a sideways drag) continues its own series.
 */
export function fillFromHandle(get: Get, source: Rect, dest: Rect): FillCell[] {
  const out: FillCell[] = [];
  const below = dest.y + dest.height - (source.y + source.height);
  const above = source.y - dest.y;
  const right = dest.x + dest.width - (source.x + source.width);
  const left = source.x - dest.x;
  if (below > 0 || above > 0) {
    for (let c = source.x; c < source.x + source.width; c++) {
      const seeds = Array.from({ length: source.height }, (_, i) => get(source.y + i, c));
      if (below > 0) {
        continueSeries(seeds, below).forEach((value, i) => out.push({ row: source.y + source.height + i, col: c, value }));
      }
      if (above > 0) {
        continueSeries([...seeds].reverse(), above).forEach((value, i) => out.push({ row: source.y - 1 - i, col: c, value }));
      }
    }
  } else if (right > 0 || left > 0) {
    for (let r = source.y; r < source.y + source.height; r++) {
      const seeds = Array.from({ length: source.width }, (_, i) => get(r, source.x + i));
      if (right > 0) {
        continueSeries(seeds, right).forEach((value, i) => out.push({ row: r, col: source.x + source.width + i, value }));
      }
      if (left > 0) {
        continueSeries([...seeds].reverse(), left).forEach((value, i) => out.push({ row: r, col: source.x - 1 - i, value }));
      }
    }
  }
  return out;
}

/**
 * Fill down / up / right / left: copy the selection's edge row or column
 * (the first row for down, the last for up, the first column for right, the
 * last for left) over the rest of the selection.
 */
export function fillEdge(get: Get, range: Rect, direction: FillDirection): FillCell[] {
  const out: FillCell[] = [];
  const vertical = direction === "down" || direction === "up";
  if (vertical ? range.height < 2 : range.width < 2) return out;
  for (let r = range.y; r < range.y + range.height; r++) {
    for (let c = range.x; c < range.x + range.width; c++) {
      const from =
        direction === "down"
          ? [range.y, c]
          : direction === "up"
            ? [range.y + range.height - 1, c]
            : direction === "right"
              ? [r, range.x]
              : [r, range.x + range.width - 1];
      if (from[0] === r && from[1] === c) continue;
      out.push({ row: r, col: c, value: get(from[0], from[1]) });
    }
  }
  return out;
}
