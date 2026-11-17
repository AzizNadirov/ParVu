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
