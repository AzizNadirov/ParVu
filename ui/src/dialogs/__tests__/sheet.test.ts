import { describe, expect, it } from "vitest";
import { cellBackground, compareCell, matchesHighlight, scalePosition, withAlpha } from "../../lib/format";
import type { FormatRule } from "../../lib/format";
import { formatTick, niceTicks, shorten } from "../../lib/chart";
import { defaultChartX, defaultChartY } from "../logic";

const colors = { primary: "#2196F3", secondary: "#4CAF50", warning: "#FF9800", error: "rgb(244, 67, 54)" };

describe("conditional formatting", () => {
  it("compares numbers as numbers and the rest as text", () => {
    expect(compareCell("10", "9")).toBeGreaterThan(0);
    expect(compareCell("b", "a")).toBeGreaterThan(0);
    expect(compareCell("2024-02-01", "2024-10-01")).toBeLessThan(0);
  });

  it("matches highlight rules", () => {
    const r = (op: Extract<FormatRule, { kind: "highlight" }>["op"], value = "", value2 = "") =>
      ({ kind: "highlight", column: "c", op, value, value2, color: "error" }) as const;
    expect(matchesHighlight(r("gt", "5"), "10")).toBe(true);
    expect(matchesHighlight(r("gt", "5"), null)).toBe(false);
    expect(matchesHighlight(r("between", "2", "4"), "3")).toBe(true);
    expect(matchesHighlight(r("between", "2", "4"), "5")).toBe(false);
    expect(matchesHighlight(r("contains", "AB"), "xaby")).toBe(true);
    expect(matchesHighlight(r("is_null"), null)).toBe(true);
    expect(matchesHighlight(r("eq", "x"), "x")).toBe(true);
  });

  it("places values on a scale and mixes colours", () => {
    expect(scalePosition("5", { min: 0, max: 10 })).toBe(0.5);
    expect(scalePosition("50", { min: 0, max: 10 })).toBe(1);
    expect(scalePosition("x", { min: 0, max: 10 })).toBeNull();
    expect(scalePosition("3", { min: 3, max: 3 })).toBe(1);
    expect(withAlpha("#fff", 0.5)).toBe("rgba(255, 255, 255, 0.5)");
    expect(withAlpha("rgb(1, 2, 3)", 0.25)).toBe("rgba(1, 2, 3, 0.25)");
    const rules: FormatRule[] = [
      { kind: "scale", column: "c", color: "primary" },
      { kind: "highlight", column: "c", op: "gt", value: "8", value2: "", color: "error" },
    ];
    expect(cellBackground(rules, "0", { c: { min: 0, max: 10 } }, colors)).toBe("rgba(33, 150, 243, 0.08)");
    expect(cellBackground(rules, "9", { c: { min: 0, max: 10 } }, colors)).toBe("rgba(244, 67, 54, 0.35)");
    expect(cellBackground(rules, null, {}, colors)).toBeNull();
  });
});

describe("chart helpers", () => {
  it("makes round ticks", () => {
    expect(niceTicks(0, 10)).toEqual([0, 2, 4, 6, 8, 10]);
    expect(niceTicks(3, 97)).toEqual([0, 20, 40, 60, 80, 100]);
    expect(niceTicks(5, 5).length).toBeGreaterThan(1);
    expect(niceTicks(-0.3, 0.7)).toEqual([-0.4, -0.2, 0, 0.2, 0.4, 0.6, 0.8]);
  });
  it("formats tick labels", () => {
    expect(formatTick(1500000)).toBe("1.5M");
    expect(formatTick(25000)).toBe("25k");
    expect(formatTick(0.25)).toBe("0.25");
    expect(formatTick(1234)).toBe("1234");
    expect(shorten("abcdefgh", 5)).toBe("abcd…");
  });
});

describe("chart defaults", () => {
  it("picks a category and a measure", () => {
    const cols = [
      { name: "id", dtype: "BIGINT" },
      { name: "region", dtype: "VARCHAR" },
      { name: "qty", dtype: "INTEGER" },
    ];
    expect(defaultChartX(cols)).toBe("region");
    expect(defaultChartY(cols, "region")).toBe("qty");
    expect(defaultChartY([{ name: "id", dtype: "BIGINT" }], "x")).toBe("id");
  });
});
