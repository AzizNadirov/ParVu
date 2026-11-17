import { describe, expect, it } from "vitest";
import type { ColumnMeta, CustomDefinition } from "../../lib/api";
import {
  buildCustomOp,
  cellFromInput,
  cellToInput,
  customArgProblems,
  definitionToSave,
  duplicateDefinition,
  expectedFromActual,
  familiesOf,
  familyOfType,
  fittingColumns,
  functionsForColumn,
  initialCustomForm,
  isNewTable,
  newDefinition,
  operationsFor,
  signatureText,
  syncFunctionExample,
} from "../logic";

const col = (name: string, dtype: string, family: ColumnMeta["family"]): ColumnMeta => ({ name, dtype, family });
const columns = [col("Phone", "VARCHAR", "text"), col("qty", "BIGINT", "integer"), col("price", "DOUBLE", "float")];

const cleanPhone: CustomDefinition = {
  schema: 1,
  kind: "function",
  name: "clean_phone",
  status: "ok",
  params: [
    { name: "phone", kind: "column", type: ["text"] },
    { name: "prefix", kind: "value", dtype: "VARCHAR", optional: true, default: "+" },
  ],
  result: { kind: "column", type: ["text"] },
  body: { language: "sql", source: "prefix || phone" },
  flags: { uses_current_time: false },
  examples: [],
  compiled: { template: "", hash: "", shape: "row", reads_other_rows: false, result_types: {}, app: "", duckdb: "" },
};

const topN: CustomDefinition = {
  schema: 1,
  kind: "operation",
  name: "top_n_per_group",
  status: "ok",
  params: [
    { name: "t", kind: "table" },
    { name: "grp", kind: "column", type: ["any"] },
    { name: "score", kind: "column", type: ["number"] },
    { name: "n", kind: "value", dtype: "BIGINT", optional: true, default: "3", min: 1 },
  ],
  result: { kind: "table" },
  placement: "step",
  body: { language: "sql", source: "SELECT * FROM t" },
  flags: { uses_current_time: false },
  examples: [],
};

describe("custom tools", () => {
  it("expands type sets and filters columns by family", () => {
    expect(familiesOf(["number"])).toEqual(["integer", "decimal", "float"]);
    expect(familiesOf([])).toContain("text");
    expect(fittingColumns(topN.params[2], columns).map((c) => c.name)).toEqual(["qty", "price"]);
    expect(familyOfType("DECIMAL(18,3)")).toBe("decimal");
    expect(familyOfType("TIMESTAMP WITH TIME ZONE")).toBe("timestamp_tz");
  });

  it("writes the signature as text", () => {
    expect(signatureText(cleanPhone)).toBe('clean_phone(phone: column<text>, prefix: value<VARCHAR> = "+") → column<text>');
    expect(signatureText(topN)).toBe(
      'top_n_per_group(t: table, grp: column<any>, score: column<number>, n: value<BIGINT> = "3") → table',
    );
  });

  it("offers functions for a column's family and operations by placement", () => {
    expect(functionsForColumn([cleanPhone, topN], "text").map((d) => d.name)).toEqual(["clean_phone"]);
    expect(functionsForColumn([cleanPhone], "integer")).toEqual([]);
    expect(operationsFor([cleanPhone, topN], false).map((d) => d.name)).toEqual(["top_n_per_group"]);
    expect(operationsFor([topN], true)).toEqual([]);
    expect(isNewTable({ ...topN, params: [...topN.params, { name: "u", kind: "table" }] })).toBe(true);
  });

  it("builds a function call with the selected column", () => {
    const form = initialCustomForm(cleanPhone, columns, "Phone");
    expect(form.args).toEqual({ phone: "Phone", prefix: "+" });
    expect(buildCustomOp(cleanPhone, form, columns)).toEqual({
      op: "custom_function",
      name: "clean_phone",
      args: { phone: { column: "Phone" }, prefix: { value: "+" } },
      output: { to: "new", name: "" },
    });
    // An empty optional value is left out, so the default applies.
    const op = buildCustomOp(cleanPhone, { ...form, args: { ...form.args, prefix: "" }, toNew: false }, columns);
    expect(op).toMatchObject({ args: { phone: { column: "Phone" } }, output: { to: "in_place" } });
    expect(op && "args" in op && "prefix" in op.args).toBe(false);
  });

  it("builds an operation and reports argument problems", () => {
    const form = initialCustomForm(topN, columns, "qty");
    // The selected column fills the first parameter it fits; the next takes another.
    expect(form.args.grp).toBe("qty");
    expect(form.args.score).toBe("price");
    expect(buildCustomOp(topN, form, columns)).toMatchObject({ op: "custom_operation", new_table: false });
    const problems = (args: Record<string, string>) => customArgProblems(topN, columns, { ...form, args: { ...form.args, ...args } });
    expect(problems({ score: "Phone" })).toEqual([["score", "custom.apply.wrong_type"]]);
    expect(problems({ score: "" })).toEqual([["score", "custom.apply.choose_column"]]);
    expect(problems({ n: "0" })).toEqual([["n", "custom.apply.out_of_range"]]);
    expect(problems({ n: "2.5" })).toEqual([["n", "custom.apply.not_a_number"]]);
    expect(problems({ n: "" })).toEqual([]);
    expect(buildCustomOp(topN, { ...form, args: { ...form.args, n: "x" } }, columns)).toBeNull();
  });

  it("keeps function examples in step with the signature", () => {
    const def = newDefinition("function");
    expect(def.examples[0].input.columns).toEqual([{ name: "x", dtype: "BIGINT" }]);
    const changed = { ...def, params: [...def.params, { name: "y", kind: "column" as const, type: ["text"] }] };
    const ex = syncFunctionExample(changed, { ...def.examples[0], input: { columns: [{ name: "x", dtype: "DOUBLE" }], rows: [["1"], ["2"]] } });
    expect(ex.input.columns).toEqual([{ name: "x", dtype: "DOUBLE" }, { name: "y", dtype: "VARCHAR" }]);
    expect(ex.input.rows).toEqual([["1", null], ["2", null]]);
    expect(ex.expected.rows).toEqual([[null], [null]]);
    const summary = syncFunctionExample({ ...changed, result: { kind: "value" } }, ex);
    expect(summary.expected.rows).toEqual([[null]]);
  });

  it("reads example cells and fills expected outputs", () => {
    expect(cellFromInput("")).toBeNull();
    expect(cellFromInput('""')).toBe("");
    expect(cellToInput("")).toBe('""');
    expect(cellToInput(null)).toBe("");
    const actual = { columns: [{ name: "region", dtype: "VARCHAR" }], rows: [["east"]] };
    expect(expectedFromActual(topN, actual)).toEqual({ columns: [{ name: "region" }], rows: [["east"]] });
    expect(expectedFromActual(cleanPhone, { columns: [{ name: "result" }], rows: [["+1"]] }).rows).toEqual([["+1"]]);
  });

  it("duplicates and cleans definitions for saving", () => {
    const copy = duplicateDefinition({ ...cleanPhone, sample: true, author: "ParVu" }, ["clean_phone_copy"]);
    expect(copy.name).toBe("clean_phone_copy_2");
    expect(copy.sample).toBe(false);
    expect(copy.compiled).toBeUndefined();
    const saved = definitionToSave({ ...cleanPhone, name: " f ", params: [{ name: "x", kind: "column", type: ["text"], dtype: "BIGINT" }] });
    expect(saved.name).toBe("f");
    expect(saved.params[0].dtype).toBeUndefined();
    expect(saved.compiled).toBeUndefined();
  });
});
