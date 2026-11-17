import { describe, expect, it } from "vitest";
import { formulaBody, formulaRefs, newFormulaName, refColors, refFor, REF_COLORS_LIGHT } from "../formula";

describe("formula references", () => {
  it("finds references with their positions", () => {
    const text = '= [qty] * sales[price] + ["unit cost"]';
    const refs = formulaRefs(text);
    expect(refs.map((r) => r.column)).toEqual(["qty", "price", "unit cost"]);
    expect(text.slice(refs[1].from, refs[1].to)).toBe("sales[price]");
    expect(text.slice(refs[2].from, refs[2].to)).toBe('["unit cost"]');
  });

  it("ignores brackets inside text", () => {
    expect(formulaRefs("CONCAT([a], ' [b] ')").map((r) => r.column)).toEqual(["a"]);
  });

  it("colours each column once, in order", () => {
    const colors = refColors(formulaRefs("[a] + [b] * [a]"));
    expect([...colors.keys()]).toEqual(["a", "b"]);
    expect(colors.get("a")).toBe(REF_COLORS_LIGHT[0]);
    expect(colors.get("b")).toBe(REF_COLORS_LIGHT[1]);
  });

  it("writes references and names", () => {
    expect(refFor("qty")).toBe("[qty]");
    expect(refFor("unit price")).toBe('["unit price"]');
    expect(newFormulaName(["column1", "x"])).toBe("column2");
    expect(formulaBody(" = [a] + 1")).toBe("[a] + 1");
  });
});

describe("ranges and errors", () => {
  it("finds ranges and tints every column in them", async () => {
    const { formulaRefs, columnHighlights, refColors } = await import("../formula");
    const refs = formulaRefs("SUM([q1:q3]) + [x]");
    expect(refs).toEqual([
      { from: 4, to: 11, column: "q1", toColumn: "q3" },
      { from: 15, to: 18, column: "x" },
    ]);
    const colors = refColors(refs);
    expect([...colors.keys()]).toEqual(["q1:q3", "x"]);
    const tints = columnHighlights(refs, ["id", "q1", "q2", "q3", "q4", "x"]);
    expect([...tints.keys()]).toEqual(["q1", "q2", "q3", "x"]);
    expect(tints.get("q2")).toBe(colors.get("q1:q3"));
    // A range written backwards covers the same columns.
    expect([...columnHighlights(formulaRefs("SUM([q3:q1])"), ["q1", "q2", "q3"]).keys()]).toEqual(["q1", "q2", "q3"]);
    expect(formulaRefs('SUM(["unit price":total])')[0]).toMatchObject({ column: "unit price", toColumn: "total" });
  });

  it("shift+click turns the reference before the cursor into a range", async () => {
    const { extendToRange } = await import("../formula");
    expect(extendToRange("SUM([q1]", 8, "q4")).toEqual({ from: 4, to: 8, insert: "[q1:q4]" });
    expect(extendToRange("SUM([q1]", 8, "unit price")).toEqual({ from: 4, to: 8, insert: '[q1:"unit price"]' });
    expect(extendToRange("SUM([q1] ", 9, "q4")).toBeNull();
    expect(extendToRange("SUM([q1:q2]", 11, "q4")).toBeNull();
  });

  it("maps a compiler error to the bar's text", async () => {
    const { errorOffset } = await import("../formula");
    expect(errorOffset("[a] +", 1, 6)).toBe(5);
    expect(errorOffset("  = [a] + )", 1, 7)).toBe(10);
    expect(errorOffset("[a]\n + )", 2, 4)).toBe(7);
    expect(errorOffset("x", 1, 99)).toBe(1);
  });
});
