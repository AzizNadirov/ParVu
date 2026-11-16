import { describe, expect, it } from "vitest";
import { buildFn, EXTRACT_FNS, FN_DIALOGS, fnLabelKey, initialValues, previewValues, TEXT_FNS } from "../colfn";
import en from "../../../../crates/parvu-config/locales/en.json";

const has = (k: string) => !!(en as Record<string, string>)[k];

describe("column functions", () => {
  it("every function and field has an English label", () => {
    for (const f of TEXT_FNS) expect(has(fnLabelKey(f)), fnLabelKey(f)).toBe(true);
    for (const n of [...EXTRACT_FNS, "add_prefix", "add_suffix", "pad", "invert"]) expect(has(`fn.${n}`), n).toBe(true);
    for (const spec of Object.values(FN_DIALOGS)) for (const f of spec!.fields) expect(has(`fn.field.${f.key}`), f.key).toBe(true);
  });

  it("builds functions from dialog values and refuses missing ones", () => {
    expect(buildFn("first", initialValues("first"))).toEqual({ fn: "first", n: 3 });
    expect(buildFn("first", { n: "0" })).toBeNull();
    expect(buildFn("before", { delimiter: "", last: false })).toBeNull();
    expect(buildFn("after", { delimiter: "@", last: true })).toEqual({ fn: "after", delimiter: "@", last: true });
    expect(buildFn("add_prefix", { text: "" })).toBeNull();
    expect(buildFn("pad", initialValues("pad"))).toEqual({ fn: "pad", length: 8, fill: "0", side: "start" });
    expect(buildFn("regex_extract", { pattern: "\\d+", group: "0" })).toEqual({ fn: "regex_extract", pattern: "\\d+", group: 0 });
  });

  it("takes distinct non-empty values from the page for previews", () => {
    const rows = [["a", "x"], ["b", null], ["a", ""], ["c", "y"]];
    expect(previewValues(rows, 1)).toEqual(["x", "y"]);
    expect(previewValues(rows, 0, 2)).toEqual(["a", "b"]);
  });
});

describe("date functions", () => {
  it("every date label, unit and filter has an English label", async () => {
    const { ADD_UNITS, DATE_FILTERS } = await import("../colfn");
    const units = ["year", "quarter", "month", "week", "day", "hour", "minute", "second"];
    for (const u of units) expect(has(`unit.${u}`) && has(`unit.${u}s`), u).toBe(true);
    for (const u of ADD_UNITS) expect(has(`unit.${u}`), u).toBe(true);
    for (const f of DATE_FILTERS) expect(has(f.key), f.key).toBe(true);
    for (let i = 1; i <= 12; i++) expect(has(`date.month.${i}`)).toBe(true);
    for (let i = 1; i <= 7; i++) expect(has(`date.day.${i}`)).toBe(true);
    // Step labels the backend builds with the unit in the key.
    for (const u of ["day", "week", "month", "quarter", "year"])
      for (const k of ["last", "this", "previous"]) expect(has(`step.date_range.${k}_${u}`), `${k}_${u}`).toBe(true);
    for (const u of units) expect(has(`step.date_diff_${u}`), u).toBe(true);
  });

  it("builds add-time and format functions", () => {
    expect(buildFn("add_time", initialValues("add_time"))).toEqual({ fn: "add_time", n: 1, unit: "day" });
    expect(buildFn("add_time", { amount: "-3", unit: "month" })).toEqual({ fn: "add_time", n: -3, unit: "month" });
    expect(buildFn("add_time", { amount: "0", unit: "day" })).toBeNull();
    expect(buildFn("format_date", initialValues("format_date"))).toEqual({ fn: "format_date", format: "%d.%m.%Y" });
  });
});

describe("JSON functions", () => {
  it("builds path and join functions and has labels", () => {
    expect(buildFn("json_path", initialValues("json_path"))).toBeNull();
    expect(buildFn("json_path", { path: "$.a.b", as_text: true })).toEqual({ fn: "json_path", path: "$.a.b", text: true });
    expect(buildFn("json_path", { path: "a.b", as_text: false })).toBeNull();
    expect(buildFn("join_list", { separator: "" })).toEqual({ fn: "join_list", separator: "" });
    for (const k of ["fn.json_path", "fn.list_length", "fn.join_list", "json.to_rows", "json.expand.title", "fn.field.path", "fn.field.as_text"])
      expect(has(k), k).toBe(true);
    for (const t of ["bigint", "double", "boolean", "varchar", "json"]) expect(has(`json.type.${t}`), t).toBe(true);
  });
});
