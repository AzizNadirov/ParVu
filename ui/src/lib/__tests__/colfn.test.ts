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
