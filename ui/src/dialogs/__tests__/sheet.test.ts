import { describe, expect, it } from "vitest";
import { cellBackground, compareCell, matchesHighlight, scalePosition, withAlpha } from "../../lib/format";
import type { FormatRule } from "../../lib/format";
import {
  OTHER_LIGHT,
  PALETTE_DARK,
  PALETTE_LIGHT,
  axisFormatter,
  chartTitle,
  columnRole,
  compactValue,
  formatTick,
  formatValue,
  isDark,
  niceTicks,
  seriesColors,
  shorten,
  suggestChart,
  switchKind,
  valueExtent,
} from "../../lib/chart";
import { defaultChartX, defaultChartY } from "../logic";
import { initialSpec } from "../ChartDialog";

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
  it("formats axes: in full under 10,000, compact above, one style per axis", () => {
    const small = axisFormatter([0, 2000, 4000, 6000]);
    expect(small(2000)).toBe("2,000");
    expect(axisFormatter([0, 0.25, 0.5])(0.25)).toBe("0.25");
    const big = axisFormatter([0, 10000, 20000, 30000]);
    expect(big(20000)).toBe("20K");
    expect(big(0)).toBe("0");
    expect(axisFormatter([0, 500000, 1000000, 1500000])(1500000)).toBe("1.5M");
    expect(formatTick(1500000)).toBe("1.5M");
    expect(formatValue(1234.567)).toBe("1,235");
    expect(formatValue(12.3456)).toBe("12.35");
    expect(formatValue(null)).toBe("—");
    expect(compactValue(12900)).toBe("12.9K");
    expect(compactValue(1284)).toBe("1,284");
    expect(shorten("abcdefgh", 5)).toBe("abcd…");
  });

  it("colours series in fixed palette order, gray for Other", () => {
    const c = seriesColors([{ other: false }, { other: false }, { other: true }], false);
    expect(c).toEqual([PALETTE_LIGHT[0], PALETTE_LIGHT[1], OTHER_LIGHT]);
    expect(seriesColors([{ other: false }], true)[0]).toBe(PALETTE_DARK[0]);
    expect(isDark("#1d1b20")).toBe(true);
    expect(isDark("rgb(255, 255, 255)")).toBe(false);
  });

  it("suggests a chart from the column", () => {
    const cols = [
      { name: "order_id", dtype: "BIGINT" },
      { name: "region", dtype: "VARCHAR" },
      { name: "day", dtype: "DATE" },
      { name: "revenue", dtype: "DOUBLE" },
    ];
    expect(suggestChart(cols)).toMatchObject({ kind: "line", x: "day", y: "revenue" });
    expect(suggestChart(cols, "region")).toMatchObject({ kind: "bar", x: "region", y: null });
    expect(suggestChart(cols, "revenue")).toMatchObject({ kind: "histogram", x: "revenue" });
    expect(suggestChart(cols, "day")).toMatchObject({ kind: "line", x: "day", y: "revenue" });
    expect(suggestChart(cols.filter((c) => c.name !== "day"))).toMatchObject({ kind: "bar", x: "region", y: "revenue" });
    expect(columnRole({ name: "order_id", dtype: "BIGINT" })).toBe("category");
    // An amount beats a count or a price, whatever the column order.
    const sales = [
      { name: "region", dtype: "VARCHAR" },
      { name: "qty", dtype: "INTEGER" },
      { name: "unit_price", dtype: "DOUBLE" },
      { name: "revenue", dtype: "DOUBLE" },
    ];
    expect(suggestChart(sales)).toMatchObject({ kind: "bar", x: "region", y: "revenue" });
    expect(suggestChart(sales.slice(0, 3))).toMatchObject({ y: "unit_price" });
  });

  it("keeps the axes valid when switching the kind of chart", () => {
    const cols = [
      { name: "order_id", dtype: "BIGINT" },
      { name: "day", dtype: "DATE" },
      { name: "region", dtype: "VARCHAR" },
      { name: "qty", dtype: "INTEGER" },
      { name: "revenue", dtype: "DOUBLE" },
    ];
    const bar = { ...initialSpec({ tabId: 1, table: "t", columns: cols }), kind: "bar" as const, x: "region", y: "revenue" };
    // Text can't go on a scatter's x: take another measure, keep y.
    expect(switchKind(bar, "scatter", cols)).toMatchObject({ kind: "scatter", x: "qty", y: "revenue" });
    // A histogram shows the measure's distribution.
    expect(switchKind(bar, "histogram", cols)).toMatchObject({ x: "revenue" });
    // Back from a histogram, group by a category (bar) or date (line).
    const hist = switchKind(bar, "histogram", cols);
    expect(switchKind(hist, "bar", cols)).toMatchObject({ x: "region", y: "revenue" });
    expect(switchKind(hist, "line", cols)).toMatchObject({ x: "day", y: "revenue" });
    // Columns that already fit are kept.
    expect(switchKind(bar, "line", cols)).toMatchObject({ x: "region", y: "revenue" });
  });

  it("titles and value extents", () => {
    const t = (k: string, a?: Record<string, unknown>) => (a ? `${k}${JSON.stringify(a)}` : k);
    const spec = { kind: "bar" as const, x: "region", y: null, agg: "sum" as const, series: "product", time_unit: "auto" as const, sort: "auto" as const, limit: 20, other: true, bins: 20 };
    expect(chartTitle(spec, t)).toBe('chart.title.by{"measure":"chart.title.rows","x":"region"}chart.title.split{"series":"product"}');
    const data = {
      x_kind: "category" as const, time_unit: null, labels: ["a", "b"], other_label: null, bin_edges: [], truncated: false,
      series: [
        { name: "x", other: false, values: [3, -1], xs: [] },
        { name: "y", other: false, values: [4, null], xs: [] },
      ],
    };
    expect(valueExtent(data, true)).toEqual([-1, 7]);
    expect(valueExtent(data, false)).toEqual([-1, 4]);
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
