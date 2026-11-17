// Column types: the icon of each type family in the grid header, and the
// "Read as" choices for a column of a CSV file.
import {
  mdiAlphabetical,
  mdiCalendar,
  mdiCalendarClock,
  mdiCalendarQuestion,
  mdiClockOutline,
  mdiCodeBraces,
  mdiCodeBrackets,
  mdiDecimal,
  mdiFunctionVariant,
  mdiFileTreeOutline,
  mdiHelpCircleOutline,
  mdiNumeric,
  mdiTimerOutline,
  mdiToggleSwitchOutline,
} from "@mdi/js";
import type { ColumnMeta, Reading, SourceColumn, TypeFamily } from "./api";

const FAMILY_ICON: Record<TypeFamily, string> = {
  integer: mdiNumeric,
  decimal: mdiDecimal,
  float: mdiDecimal,
  text: mdiAlphabetical,
  boolean: mdiToggleSwitchOutline,
  date: mdiCalendar,
  timestamp: mdiCalendarClock,
  timestamp_tz: mdiCalendarClock,
  time: mdiClockOutline,
  interval: mdiTimerOutline,
  json: mdiCodeBraces,
  list: mdiCodeBrackets,
  struct: mdiFileTreeOutline,
  map: mdiFileTreeOutline,
  binary: mdiHelpCircleOutline,
  other: mdiHelpCircleOutline,
};

/** Header icon name of a column: its family, or a question mark on dates
 * whose day and month order was a guess. */
export function headerIconName(c: ColumnMeta, source?: SourceColumn): string {
  if (source?.ambiguous) return "pv-ambiguous";
  return `pv-${c.family ?? "other"}`;
}

/** Glide's header icon sprites, one per family plus the ambiguous date. */
export function headerIcons(): Record<string, (p: { fgColor: string; bgColor: string }) => string> {
  const sprite = (path: string) => (p: { fgColor: string }) =>
    `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24"><path fill="${p.fgColor}" d="${path}"/></svg>`;
  const map: Record<string, (p: { fgColor: string; bgColor: string }) => string> = {};
  for (const [family, path] of Object.entries(FAMILY_ICON)) map[`pv-${family}`] = sprite(path);
  map["pv-ambiguous"] = sprite(mdiCalendarQuestion);
  map["pv-formula"] = sprite(mdiFunctionVariant);
  return map;
}

/** `%d/%m/%Y` ↔ `%m/%d/%Y`; unchanged when it has no day and month. */
export function swapDayMonth(format: string): string {
  if (!format.includes("%d") || !format.includes("%m")) return format;
  return format.replace("%d", "\u0001").replace("%m", "%d").replace("\u0001", "%m");
}

/** Whether a date format puts the day before the month. */
export const dayFirst = (format: string) => format.indexOf("%d") < format.indexOf("%m");

/** A "Read as" choice: a translation key (with args) and the reading. */
export interface ReadAsChoice {
  key: string;
  args?: Record<string, string>;
  reading: Reading;
}

/** The ways a CSV column can be read, in menu order. */
export function readAsChoices(): ReadAsChoice[] {
  const date = (format: string, example: string): ReadAsChoice => ({
    key: "types.read.date_format",
    args: { example },
    reading: { kind: "date_format", format },
  });
  return [
    { key: "types.read.text", reading: { kind: "text" } },
    { key: "types.read.integer", reading: { kind: "integer" } },
    { key: "types.read.number", args: { example: "1234.5" }, reading: { kind: "number" } },
    { key: "types.read.number", args: { example: "1,234.5" }, reading: { kind: "locale_number", thousands: ",", decimal: "." } },
    { key: "types.read.number", args: { example: "1.234,5" }, reading: { kind: "locale_number", thousands: ".", decimal: "," } },
    {
      key: "types.read.number",
      args: { example: "1 234,5" },
      reading: { kind: "locale_number", thousands: "   ", decimal: "," },
    },
    { key: "types.read.boolean", reading: { kind: "boolean" } },
    { key: "types.read.date_iso", reading: { kind: "date" } },
    date("%d.%m.%Y", "31.12.2024"),
    date("%d/%m/%Y", "31/12/2024"),
    date("%m/%d/%Y", "12/31/2024"),
    { key: "types.read.timestamp", reading: { kind: "timestamp" } },
    { key: "types.read.time", reading: { kind: "time" } },
    { key: "types.read.json", reading: { kind: "json" } },
  ];
}

/** Conversion targets: the "Read as" choices plus Unix time. */
export function convertChoices(): ReadAsChoice[] {
  const epoch = (unit: "seconds" | "milliseconds" | "microseconds"): ReadAsChoice => ({
    key: `types.read.epoch_${unit}`,
    reading: { kind: "epoch", unit },
  });
  return [...readAsChoices(), epoch("seconds"), epoch("milliseconds"), epoch("microseconds")];
}

/** Same reading (for the check mark in the menu). */
export function sameReading(a: Reading, b: Reading): boolean {
  // Field order may differ between the backend and the choices list.
  const canon = (r: Reading) => JSON.stringify(Object.entries(r).sort(([x], [y]) => x.localeCompare(y)));
  return canon(a) === canon(b);
}
