import { describe, expect, it, vi } from "vitest";
import type { ColumnStats, FunctionDef, SearchHit } from "../../lib/api";
import {
  SearchNavigator,
  buildAppendOp,
  buildDropDuplicatesOp,
  buildDropNullOp,
  buildExportOptions,
  buildJoinOp,
  buildReplaceRequest,
  clampInt,
  columnRef,
  datesApplicable,
  defaultMathExpression,
  defaultReplaceOutput,
  exportFormatOf,
  exportValidation,
  fileTypeForPath,
  filterFunctions,
  filterUniqueValues,
  fmtNum,
  fmtPct,
  formatSignature,
  groupFunctions,
  initialExportState,
  initialSampleSize,
  insertAt,
  isTemporal,
  isTruthySetting,
  joinPath,
  mailtoUrl,
  parseCssVariables,
  previewValue,
  sortUniqueValues,
  statsRows,
  themeFileName,
  validTableVar,
  withExtension,
} from "../logic";
import type { SearchParams } from "../logic";

describe("operations", () => {
  it("builds a join", () => {
    expect(buildJoinOp("b", "left", "id", "bid")).toEqual({
      op: "join",
      right: "b",
      join_type: "left",
      left_key: "id",
      right_key: "bid",
    });
    expect(buildJoinOp("", "inner", "id", "id")).toBeNull();
    expect(buildJoinOp("b", "inner", "id", "")).toBeNull();
  });

  it("builds an append", () => {
    expect(buildAppendOp(["b", "c"], true)).toEqual({ op: "append", tables: ["b", "c"], distinct: true });
    expect(buildAppendOp([], false)).toBeNull();
  });

  it("keeps column order for drop duplicates", () => {
    expect(buildDropDuplicatesOp(["a", "b", "c"], new Set(["c", "a"]), "last")).toEqual({
      op: "drop_duplicates",
      columns: ["a", "c"],
      keep: "last",
    });
    expect(buildDropDuplicatesOp(["a"], [], "first")).toEqual({ op: "drop_duplicates", columns: [], keep: "first" });
  });

  it("builds drop null with and without a sentinel", () => {
    expect(buildDropNullOp("x", false, "-1")).toEqual({ op: "drop_null", column: "x", null_value: null });
    expect(buildDropNullOp("x", true, "-1")).toEqual({ op: "drop_null", column: "x", null_value: "-1" });
    expect(buildDropNullOp("x", true, "")).toEqual({ op: "drop_null", column: "x", null_value: "" });
    expect(buildDropNullOp("", false, "")).toBeNull();
  });

  it("builds a replace request", () => {
    expect(defaultReplaceOutput("name")).toBe("name_replaced");
    const base = { column: "a", pattern: "x", replacement: "", output: " a_out ", caseSensitive: true, regex: false };
    expect(buildReplaceRequest(base)).toEqual({
      column: "a",
      pattern: "x",
      replacement: "",
      output_column: "a_out",
      case_sensitive: true,
      regex: false,
    });
    expect(buildReplaceRequest({ ...base, pattern: "" })).toBeNull();
    expect(buildReplaceRequest({ ...base, output: "  " })).toBeNull();
  });
});

describe("unique values", () => {
  it("sorts case-insensitively with NULL first", () => {
    expect(sortUniqueValues(["b", null, "A", "a", "C"])).toEqual([null, "A", "a", "b", "C"]);
  });

  it("filters by substring, NULL matching 'null'", () => {
    const v = [null, "Apple", "banana", "nullable"];
    expect(filterUniqueValues(v, "AN")).toEqual(["banana"]);
    expect(filterUniqueValues(v, "null")).toEqual([null, "nullable"]);
    expect(filterUniqueValues(v, "  ")).toBe(v);
  });
});

describe("column stats", () => {
  it("formats numbers like Python's {:,.6g}", () => {
    expect(fmtNum(1234567)).toBe("1,234,567");
    expect(fmtNum(3.14159265)).toBe("3.14159");
    expect(fmtNum(12345.678)).toBe("12,345.7");
    expect(fmtNum(1234567.89)).toBe("1.23457e+06");
    expect(fmtNum(0.000012345)).toBe("1.2345e-05");
    expect(fmtNum(null)).toBe("");
  });

  it("formats percentages", () => {
    expect(fmtPct(1, 0)).toBe("0%");
    expect(fmtPct(0, 10)).toBe("0%");
    expect(fmtPct(1, 100000)).toBe("<0.01%");
    expect(fmtPct(1, 3)).toBe("33.33%");
  });

  it("adds mean and std only when present", () => {
    const s: ColumnStats = {
      column: "c",
      dtype: "VARCHAR",
      row_count: 4,
      non_null: 3,
      null: 1,
      distinct: 2,
      min: "a",
      max: null,
      mean: null,
      std: null,
    };
    const rows = statsRows(s);
    expect(rows.map((r) => r[0])).not.toContain("column_stats.mean");
    expect(rows.find((r) => r[0] === "column_stats.null")?.[1]).toBe("1 (25.00%)");
    expect(rows.find((r) => r[0] === "column_stats.max")?.[1]).toBe("—");
    expect(statsRows({ ...s, mean: 2.5, std: 0.5 }).map((r) => r[0])).toContain("column_stats.std");
  });
});

describe("export", () => {
  it("detects the file type from the extension", () => {
    expect(fileTypeForPath("/a/b.CSV")).toBe("csv");
    expect(fileTypeForPath("x.tsv")).toBe("tsv");
    expect(fileTypeForPath("x.pq")).toBe("parquet");
    expect(fileTypeForPath("x.ndjson")).toBe("jsonl");
    expect(fileTypeForPath("x.xlsx")).toBe("xlsx");
    expect(fileTypeForPath("x.weird")).toBeNull();
    expect(fileTypeForPath("/dir.d/noext")).toBeNull();
    expect(fileTypeForPath(".hidden")).toBeNull();
  });

  it("maps file types to backend formats", () => {
    expect(exportFormatOf("tsv")).toBe("csv");
    expect(exportFormatOf("jsonl")).toBe("json");
    expect(exportFormatOf("xlsx")).toBe("excel");
  });

  it("swaps or adds extensions", () => {
    expect(withExtension("/x/data.csv", "parquet")).toBe("/x/data.parquet");
    expect(withExtension("/x.y/data", "json")).toBe("/x.y/data.json");
    expect(withExtension("C:\\out\\t.csv", "xlsx")).toBe("C:\\out\\t.xlsx");
    expect(withExtension("", "csv")).toBe("export.csv");
    expect(joinPath("/home/u", "a.csv")).toBe("/home/u/a.csv");
    expect(joinPath("/home/u/", "a.csv")).toBe("/home/u/a.csv");
    expect(joinPath(undefined, "a.csv")).toBe("a.csv");
  });

  it("recognises temporal types", () => {
    expect(isTemporal("DATE")).toBe(true);
    expect(isTemporal("timestamp with time zone")).toBe(true);
    expect(isTemporal("TIME")).toBe(true);
    expect(isTemporal("VARCHAR")).toBe(false);
  });

  const cols = [
    { name: "id", dtype: "BIGINT" },
    { name: "d", dtype: "DATE" },
  ];

  it("builds export options", () => {
    const s = initialExportState("tbl", cols, "/out", "snappy");
    expect(s.path).toBe("/out/tbl.csv");
    s.columns[0].cast = "VARCHAR";
    s.dateFormat = " %d/%m/%Y ";
    expect(buildExportOptions(s)).toEqual({
      path: "/out/tbl.csv",
      format: "csv",
      columns: [
        { name: "id", source_type: "BIGINT", include: true, cast_to: "VARCHAR" },
        { name: "d", source_type: "DATE", include: true, cast_to: "" },
      ],
      date_format: "%d/%m/%Y",
      timestamp_format: "",
      delimiter: ",",
      include_header: true,
      quoting: "needed",
      compression: "snappy",
      json_lines: false,
    });
  });

  it("drops date formats for parquet and sets tsv / jsonl specifics", () => {
    const s = { ...initialExportState("t", cols, undefined, "zstd"), dateFormat: "%Y" };
    expect(datesApplicable("parquet", s.columns)).toBe(false);
    expect(buildExportOptions({ ...s, type: "parquet" }).date_format).toBe("");
    expect(buildExportOptions({ ...s, type: "tsv", delimiter: ";" }).delimiter).toBe("\t");
    const j = buildExportOptions({ ...s, type: "jsonl" });
    expect(j.format).toBe("json");
    expect(j.json_lines).toBe(true);
    expect(datesApplicable("csv", [{ dtype: "INTEGER" }])).toBe(false);
  });

  it("validates", () => {
    const s = initialExportState("t", cols, undefined, "snappy");
    expect(exportValidation(s)).toBeNull();
    expect(exportValidation({ ...s, path: " " })).toBe("no_path");
    expect(exportValidation({ ...s, columns: s.columns.map((c) => ({ ...c, include: false })) })).toBe("no_columns");
  });
});

describe("expressions", () => {
  it("quotes column references when needed", () => {
    expect(columnRef("t", "price")).toBe("t[price]");
    expect(columnRef("t", "unit price")).toBe('t["unit price"]');
    expect(columnRef("t", 'a"b')).toBe('t["a\\"b"]');
    expect(defaultMathExpression("t", ["a", "b c"])).toBe('t[a] + t["b c"]');
    expect(defaultMathExpression("t", ["a"])).toBe("");
  });

  const fn = (name: string, desc = ""): FunctionDef => ({
    name,
    params: [
      { name: "x", logical_type: "ANY", required: true, description: "" },
      { name: "y", logical_type: "ANY", required: false, description: "" },
    ],
    return_type: "ANY",
    description: desc,
    example: "",
    sql_name: name.toLowerCase(),
  });

  it("formats signatures", () => {
    expect(formatSignature(fn("ROUND"))).toBe("ROUND(x, [y])");
  });

  it("groups and filters functions", () => {
    const fs = [fn("UPPER", "upper case"), fn("SUM"), fn("ZZZ")];
    const groups = groupFunctions(fs);
    expect(groups.map(([k]) => k)).toEqual(["expr.aggregates", "expr.text", "dialog.expr_help.other"]);
    expect(filterFunctions(fs, "case").map((f) => f.name)).toEqual(["UPPER"]);
    expect(filterFunctions(fs, "zz").map((f) => f.name)).toEqual(["ZZZ"]);
  });

  it("inserts text at the caret", () => {
    expect(insertAt("ab", 1, 1, "X()", 1)).toEqual(["aX()b", 3]);
    expect(insertAt("abc", 0, 3, "z")).toEqual(["z", 1]);
  });
});

describe("misc", () => {
  it("clamps sample sizes", () => {
    expect(initialSampleSize(500, 20)).toBe(20);
    expect(initialSampleSize(5000, 1e6)).toBe(1000);
    expect(initialSampleSize(500, 0)).toBe(1);
    expect(clampInt(NaN, 1, 9)).toBe(1);
    expect(clampInt(12.6, 1, 9)).toBe(9);
  });

  it("reads settings values", () => {
    expect(isTruthySetting("True")).toBe(true);
    expect(isTruthySetting("false")).toBe(false);
    expect(isTruthySetting(true)).toBe(true);
    expect(validTableVar("data", ["SELECT"])).toBe(true);
    expect(validTableVar(" select ", ["SELECT"])).toBe(false);
    expect(validTableVar("  ", [])).toBe(false);
  });

  it("parses theme CSS variables", () => {
    const vars = parseCssVariables(':root {\n  --pv-colors-background: #FFF;\n  --pv-layout-code-font-family: "Courier New";\n}\n');
    expect(vars).toEqual({ "--pv-colors-background": "#FFF", "--pv-layout-code-font-family": '"Courier New"' });
    expect(themeFileName("ParVu Light")).toBe("parvu_light.json");
  });

  it("builds a mailto link", () => {
    expect(mailtoUrl("a@b.c", "Crash report", "x&y", 2)).toBe("mailto:a@b.c?subject=Crash%20report&body=x%26");
  });

  it("shortens long values", () => {
    expect(previewValue("abc", 10)).toBe("abc");
    expect(previewValue("abcdefghijkl", 10)).toBe("abcdefg...");
  });
});

describe("SearchNavigator", () => {
  const params: SearchParams = { query: "x", column: null, case_sensitive: false, whole_cell: false, regex: false };
  const hit = (row: number, col = 0): SearchHit => ({ row, column_index: col, column: `c${col}`, value: "x" });

  it("walks forward from the anchor, then through the cache", async () => {
    const results = [hit(3), hit(7), null];
    const find = vi.fn(async () => results.shift() ?? null);
    const nav = new SearchNavigator(find);
    const anchor = { row: 1, col: 0 };

    const a = await nav.next(params, anchor);
    expect(a).toMatchObject({ kind: "hit", current: 1, total: 1, more: true });
    expect(find).toHaveBeenLastCalledWith(expect.objectContaining({ direction: "next", after_row: 1, after_col: 0 }));

    const b = await nav.next(params, anchor);
    expect(b).toMatchObject({ kind: "hit", current: 2, total: 2 });
    // the last hit is the next anchor
    expect(find).toHaveBeenLastCalledWith(expect.objectContaining({ after_row: 3, after_col: 0 }));

    expect(await nav.next(params, anchor)).toEqual({ kind: "last", current: 2, total: 2 });
    expect(await nav.next(params, anchor)).toEqual({ kind: "last", current: 2, total: 2 });
    expect(find).toHaveBeenCalledTimes(3);

    // back through the cache without querying
    expect(await nav.prev(params, anchor)).toMatchObject({ kind: "hit", current: 1, more: false });
    expect(find).toHaveBeenCalledTimes(3);
  });

  it("queries backwards before the first cached hit", async () => {
    const find = vi.fn(async (o: { direction?: string }) => (o.direction === "prev" ? null : hit(5, 2)));
    const nav = new SearchNavigator(find);
    await nav.next(params, { row: null, col: null });
    expect(await nav.prev(params, { row: null, col: null })).toEqual({ kind: "first", total: 1 });
    expect(find).toHaveBeenLastCalledWith(expect.objectContaining({ direction: "prev", after_row: 5, after_col: 2 }));
  });

  it("reports no matches and resets when parameters change", async () => {
    const find = vi.fn(async () => null);
    const nav = new SearchNavigator(find);
    expect(await nav.next(params, { row: null, col: null })).toEqual({ kind: "none" });
    await nav.next({ ...params, query: "y" }, { row: null, col: null });
    expect(find).toHaveBeenCalledTimes(2);
  });
});
