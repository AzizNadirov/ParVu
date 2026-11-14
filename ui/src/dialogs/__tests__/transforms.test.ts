import { describe, expect, it } from "vitest";
import {
  buildBinOp,
  buildFillNullOp,
  buildFilterOp,
  buildGroupByOp,
  buildMeltOp,
  buildPivotOp,
  buildSelectColumnsOp,
  buildSliceOp,
  buildSortByOp,
  buildSplitOp,
  buildValueCountsOp,
  buildWindowOp,
  condArity,
  defaultWindowOutput,
  emptyCondition,
  isNumericType,
  isTextType,
  moveItem,
  parseNumberList,
  windowUses,
} from "../logic";
import type { WindowSpec } from "../../lib/api";

describe("reshaping builders", () => {
  it("group by needs complete aggregations", () => {
    expect(buildGroupByOp(["r"], [{ column: "", func: "rows", alias: " " }])).toEqual({
      op: "group_by",
      by: ["r"],
      aggregations: [{ column: "", func: "rows", alias: null }],
    });
    expect(buildGroupByOp(["r"], [{ column: "", func: "sum", alias: null }])).toBeNull();
    expect(buildGroupByOp([], [])).toBeNull();
    expect(buildGroupByOp([], [{ column: "q", func: "rows", alias: "n" }])).toMatchObject({
      aggregations: [{ column: "", func: "rows", alias: "n" }],
    });
  });

  it("pivot, melt, value counts", () => {
    expect(buildPivotOp(["r"], "p", "q", "sum")).toEqual({ op: "pivot", index: ["r"], columns: "p", values: "q", func: "sum" });
    expect(buildPivotOp(["p"], "p", "q", "sum")).toBeNull();
    expect(buildPivotOp([], "p", "", "rows")).toMatchObject({ values: null });
    expect(buildPivotOp([], "p", "", "mean")).toBeNull();
    expect(buildMeltOp(["id"], [], " variable ", "value")).toEqual({
      op: "melt",
      id_columns: ["id"],
      value_columns: [],
      variable_name: "variable",
      value_name: "value",
    });
    expect(buildMeltOp(["id"], [], "id", "value")).toBeNull();
    expect(buildMeltOp([], [], "x", "x")).toBeNull();
    expect(buildValueCountsOp([], true, false)).toBeNull();
    expect(buildValueCountsOp(["a"], true, false)).toEqual({ op: "value_counts", columns: ["a"], normalize: true, dropna: false });
  });
});

describe("filter and clean builders", () => {
  const isText = (c: string) => c === "name";
  it("filter checks values by arity and column type", () => {
    expect(condArity("is_null")).toBe(0);
    expect(condArity("between")).toBe(2);
    expect(buildFilterOp([emptyCondition("name")], "and", isText)).not.toBeNull(); // = '' on text is allowed
    expect(buildFilterOp([emptyCondition("qty")], "and", isText)).toBeNull();
    expect(buildFilterOp([{ ...emptyCondition("qty"), op: "is_null" }], "or", isText)).toMatchObject({ combine: "or" });
    expect(buildFilterOp([{ ...emptyCondition("qty"), op: "between", value: "1" }], "and", isText)).toBeNull();
    expect(buildFilterOp([{ ...emptyCondition("name"), op: "in" }], "and", isText)).toBeNull();
    expect(buildFilterOp([], "and", isText)).toBeNull();
  });

  it("fill, sort, columns, slice, split, bin", () => {
    expect(buildFillNullOp(["a"], "value", "")).toBeNull();
    expect(buildFillNullOp(["a"], "forward", "x")).toEqual({ op: "fill_null", columns: ["a"], method: "forward", value: "" });
    expect(buildFillNullOp([], "mean", "")).toBeNull();

    const k = (column: string) => ({ column, ascending: true, nulls_first: false });
    expect(buildSortByOp([k("a"), k("b")])).toEqual({ op: "sort_by", keys: [k("a"), k("b")] });
    expect(buildSortByOp([k("a"), k("a")])).toBeNull();
    expect(moveItem(["a", "b", "c"], 2, -1)).toEqual(["a", "c", "b"]);
    expect(moveItem(["a", "b"], 0, -1)).toEqual(["a", "b"]);
    expect(buildSelectColumnsOp(["c", "a", "b"], new Set(["a", "c"]))).toEqual({ op: "select_columns", columns: ["c", "a"] });
    expect(buildSelectColumnsOp(["a"], new Set())).toBeNull();

    expect(buildSliceOp("head", 10, "9")).toEqual({ op: "slice", mode: "head", n: 10, seed: null });
    expect(buildSliceOp("sample", 10, "9")).toEqual({ op: "slice", mode: "sample", n: 10, seed: 9 });
    expect(buildSliceOp("sample", 10, "x")).toBeNull();
    expect(buildSliceOp("tail", 0, "")).toBeNull();

    expect(buildSplitOp("a", "-", false, null, false)).toMatchObject({ parts: null });
    expect(buildSplitOp("a", "", false, null, false)).toBeNull();
    expect(buildSplitOp("a", "-", false, 99, false)).toBeNull();

    expect(parseNumberList("0, 1.5; 10")).toEqual([0, 1.5, 10]);
    expect(parseNumberList("1, x")).toBeNull();
    expect(buildBinOp("q", "edges", 0, "0, 10, 20", "lo, hi", "b")).toEqual({
      op: "bin",
      column: "q",
      method: "edges",
      bins: 0,
      edges: [0, 10, 20],
      labels: ["lo", "hi"],
      output: "b",
    });
    expect(buildBinOp("q", "edges", 0, "0, 10, 20", "only one", "b")).toBeNull();
    expect(buildBinOp("q", "quantile", 4, "", "", "b")).toMatchObject({ bins: 4, edges: [] });
    expect(buildBinOp("q", "equal_width", 0, "", "", "b")).toBeNull();
  });

  it("column types", () => {
    expect(isTextType("VARCHAR")).toBe(true);
    expect(isTextType("INTEGER")).toBe(false);
    expect(isNumericType("DECIMAL(10,2)")).toBe(true);
    expect(isNumericType("DATE")).toBe(false);
  });
});

describe("window builder", () => {
  const spec = (patch: Partial<WindowSpec>): WindowSpec => ({
    column: "q",
    func: "cum_sum",
    periods: 1,
    window: 3,
    rank_method: "average",
    ascending: true,
    partition_by: [],
    order_by: null,
    output: "q_cumsum",
    ...patch,
  });
  it("keeps only the settings the function uses", () => {
    expect(buildWindowOp(spec({}))).toMatchObject({ op: "window", periods: null, window: null });
    expect(buildWindowOp(spec({ func: "shift", periods: -2 }))).toMatchObject({ periods: -2, window: null });
    expect(buildWindowOp(spec({ func: "shift", periods: 0 }))).toBeNull();
    expect(buildWindowOp(spec({ func: "rolling_mean", window: 0 }))).toBeNull();
    expect(buildWindowOp(spec({ func: "row_number", column: "" }))).toMatchObject({ column: "" });
    expect(buildWindowOp(spec({ column: "" }))).toBeNull();
    expect(windowUses("rank")).toEqual({ periods: false, window: false, rank: true });
    expect(defaultWindowOutput("cum_sum", "qty")).toBe("qty_cumsum");
    expect(defaultWindowOutput("row_number", "qty")).toBe("row_number");
  });
});
