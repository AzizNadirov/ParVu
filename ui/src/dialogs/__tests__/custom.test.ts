import { describe, expect, it } from "vitest";
import type { ColumnMeta, CustomDefinition } from "../../lib/api";
import {
  buildCustomOp,
  cellFromInput,
  customBodySchema,
  cellToInput,
  customArgProblems,
  definitionToSave,
  duplicateDefinition,
  expectedFromActual,
  formatRequiredColumns,
  parseRequiredColumns,
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
  aggChoice,
  buildGroupByOp,
  columnCount,
  customAggregates,
  customArgs,
  listColumns,
  newExample,
  setListSize,
  stepsDescription,
  withAggChoice,
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

  it("reads and writes required columns", () => {
    const cols = parseRequiredColumns("amount: number, region ,");
    expect(cols).toEqual([{ name: "amount", type: ["number"] }, { name: "region" }]);
    expect(formatRequiredColumns(cols)).toBe("amount: number, region");
  });

  it("completes an expression body with the parameters and the built-ins", () => {
    const builtins = [
      { name: "UPPER", params: [], return_type: "TEXT", description: "", example: "", sql_name: "" },
      { name: "weighted_average", params: [], return_type: "NUMERIC", description: "", example: "", sql_name: "", custom: true },
    ];
    const expr = customBodySchema({ ...cleanPhone, body: { language: "expression", source: "" } }, builtins);
    expect(expr.bare).toEqual(["phone", "prefix"]);
    expect(expr.functions.map((f) => f.name)).toEqual(["UPPER"]);
    const sql = customBodySchema(cleanPhone, builtins);
    expect(sql.tables).toEqual([{ name: "params", columns: ["phone", "prefix"] }]);
    expect(sql.bare).toBeUndefined();
    expect(customBodySchema(topN, builtins).tables).toEqual([{ name: "t", columns: ["grp", "score"] }]);
  });

  it("saves operations as SQL bodies", () => {
    const op = definitionToSave({ ...topN, body: { language: "expression", source: "SELECT 1" } });
    expect(op.body.language).toBe("sql");
    expect(definitionToSave({ ...cleanPhone, body: { language: "expression", source: "[phone]" } }).body.language).toBe("expression");
  });

  it("takes several columns for a columns parameter", () => {
    const dedupe: CustomDefinition = {
      ...topN,
      name: "dedupe_by",
      params: [
        { name: "t", kind: "table" },
        { name: "keys", kind: "columns", type: ["any"], min: 1 },
        { name: "latest", kind: "column", type: ["number"] },
      ],
    };
    expect(signatureText(dedupe)).toBe("dedupe_by(t: table, keys: columns<any>, latest: column<number>) → table");
    expect(signatureText({ ...dedupe, params: [dedupe.params[0], { ...dedupe.params[1], min: 2, max: 3 }] })).toContain("keys: columns<any> × 2…3");
    expect(columnCount({ name: "k", kind: "columns" })).toEqual([1, null]);
    // The selected column starts the list; the next parameter takes another column.
    const form = initialCustomForm(dedupe, columns, "Phone");
    expect(form.lists.keys).toEqual(["Phone"]);
    expect(form.args.latest).toBe("qty");
    expect(customArgs(dedupe, { ...form, lists: { keys: ["Phone", "price"] } }).keys).toEqual({ columns: ["Phone", "price"] });
    const problems = (keys: string[], p = dedupe) => customArgProblems(p, columns, { ...form, lists: { keys } });
    expect(problems([])).toEqual([["keys", "custom.apply.column_count"]]);
    expect(problems(["nope"])).toEqual([["keys", "custom.apply.choose_column"]]);
    const numbers = { ...dedupe, params: [dedupe.params[0], { ...dedupe.params[1], type: ["number"], max: 1 }, dedupe.params[2]] };
    expect(problems(["Phone"], numbers)).toEqual([["keys", "custom.apply.wrong_type"]]);
    expect(problems(["qty", "price"], numbers)).toEqual([["keys", "custom.apply.column_count"]]);
    expect(buildCustomOp(dedupe, form, columns)).toMatchObject({ args: { keys: { columns: ["Phone"] }, latest: { column: "qty" } } });
    // Operation examples name the columns as a list.
    expect(newExample(dedupe, 1).args).toEqual({ keys: ["keys"], latest: "latest" });
    // Function examples: one input column per listed column.
    const join: CustomDefinition = { ...cleanPhone, params: [{ name: "parts", kind: "columns", type: ["text"], min: 2 }], result: { kind: "column", type: ["text"] } };
    const ex = syncFunctionExample(join, { name: "e", input: { columns: [], rows: [[]] }, expected: { columns: [{ name: "result" }], rows: [[null]] } });
    expect(ex.args).toEqual({ parts: ["parts_1", "parts_2"] });
    expect(ex.input.columns).toEqual([{ name: "parts_1", dtype: "VARCHAR" }, { name: "parts_2", dtype: "VARCHAR" }]);
    const three = setListSize(join, { ...ex, input: { ...ex.input, rows: [["a", "b"]] } }, join.params[0], 3);
    expect(listColumns(join.params[0], three)).toEqual(["parts_1", "parts_2", "parts_3"]);
    expect(three.input.rows).toEqual([["a", "b", null]]);
    expect(functionsForColumn([join], "text").map((d) => d.name)).toEqual(["clean_phone"]);
    // Saved with its count; values' fields dropped.
    const saved = definitionToSave({ ...dedupe, params: [dedupe.params[0], { ...dedupe.params[1], max: 2, dtype: "BIGINT" }] });
    expect(saved.params[1]).toEqual({ name: "keys", kind: "columns", type: ["any"], min: 1, max: 2 });
  });

  it("offers summary functions of one column as Group By aggregations", () => {
    const pct: CustomDefinition = {
      ...cleanPhone,
      name: "percentile_value",
      params: [
        { name: "x", kind: "column", type: ["number"] },
        { name: "p", kind: "value", dtype: "DOUBLE", optional: true, default: "0.9" },
      ],
      result: { kind: "value", type: ["number"] },
    };
    const two = { ...pct, name: "weighted", params: [pct.params[0], { name: "w", kind: "column" as const }] };
    expect(customAggregates([cleanPhone, pct, two, { ...pct, name: "draft", status: "draft" }]).map((d) => d.name)).toEqual(["percentile_value"]);
    const a = withAggChoice({ column: "qty", func: "sum", alias: null }, "custom:percentile_value");
    expect(a).toEqual({ column: "qty", func: "custom", alias: null, custom: { name: "percentile_value", values: {} } });
    expect(aggChoice(a)).toBe("custom:percentile_value");
    expect(withAggChoice(a, "mean")).toEqual({ column: "qty", func: "mean", alias: null });
    const op = buildGroupByOp(["region"], [{ ...a, custom: { name: "percentile_value", values: { p: " " } } }]);
    expect(op).toEqual({
      op: "group_by",
      by: ["region"],
      aggregations: [{ column: "qty", func: "custom", alias: null, custom: { name: "percentile_value", values: {} } }],
    });
    expect(buildGroupByOp([], [{ column: "qty", func: "custom", alias: null }])).toBeNull();
  });

  it("describes an operation made from steps", () => {
    expect(stepsDescription(["Filter a", "Sort b"])).toBe("Filter a; Sort b");
    expect(stepsDescription(["x".repeat(400)]).length).toBe(300);
  });
});
