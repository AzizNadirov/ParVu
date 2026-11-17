import { describe, expect, it } from "vitest";
import { bareRef, bareRefPrefix, functionDetail, signature } from "../exprComplete";
import type { FunctionDef } from "../api";

const fn = (patch: Partial<FunctionDef>): FunctionDef => ({
  name: "f",
  params: [],
  return_type: "NUMERIC",
  description: "",
  example: "",
  sql_name: "",
  ...patch,
});

describe("expression completions", () => {
  it("finds a bare [ but not table[ or a range", () => {
    expect(bareRefPrefix("= [")).toBe("");
    expect(bareRefPrefix("SUM([pr")).toBe("pr");
    expect(bareRefPrefix('[x] + ["unit p')).toBe("unit p");
    expect(bareRefPrefix("sales[pr")).toBeNull();
    expect(bareRefPrefix("[a] + 1")).toBeNull();
    expect(bareRefPrefix("SUM([q1:q")).toBeNull();
  });

  it("quotes unusual names", () => {
    expect(bareRef("price")).toBe("price]");
    expect(bareRef("unit price")).toBe('"unit price"]');
  });

  it("shows a custom function's types and kind", () => {
    const f = fn({
      name: "clean_phone",
      custom: true,
      return_type: "TEXT",
      params: [
        { name: "phone", logical_type: "TEXT", required: true, description: "", accepts: ["TEXT"] },
        { name: "prefix", logical_type: "TEXT", required: false, description: "" },
      ],
    });
    expect(signature(f)).toBe("clean_phone(phone: TEXT, prefix?) → TEXT");
    expect(functionDetail(f)).toBe("custom · TEXT");
    expect(functionDetail({ ...f, aggregate: true })).toBe("custom · total · TEXT");
    expect(functionDetail(fn({ return_type: "TEXT" }))).toBe("TEXT");
  });
});
