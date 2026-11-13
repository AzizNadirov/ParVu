import { describe, expect, it } from "vitest";
import { formatRows, pythonTuple } from "../copy";

const rows = [
  ["1", "a,b", null],
  ["2", 'say "hi"', "x|y"],
];

describe("formatRows", () => {
  it("TSV", () => {
    expect(formatRows(rows, "tsv")).toBe('1\ta,b\t\n2\tsay "hi"\tx|y');
    expect(formatRows([["a\tb"]], "tsv")).toBe("a b");
  });
  it("TSV with headers", () => {
    expect(formatRows([["1"]], "tsv", ["id"])).toBe("id\n1");
  });
  it("CSV quotes when needed", () => {
    expect(formatRows(rows, "csv")).toBe('1,"a,b",\n2,"say ""hi""",x|y');
  });
  it("Markdown escapes pipes", () => {
    expect(formatRows(rows, "markdown", ["id", "text", "other"])).toBe(
      ["| id | text | other |", "| --- | --- | --- |", "| 1 | a,b |  |", '| 2 | say "hi" | x\\|y |'].join("\n"),
    );
  });
});

describe("pythonTuple", () => {
  it("formats like Python repr", () => {
    expect(pythonTuple(["1", "2.5", null], true)).toBe("(1, 2.5, None)");
    expect(pythonTuple(["a", "it's"], false)).toBe(`('a', "it's")`);
    expect(pythonTuple(["5"], false)).toBe("('5')");
    expect(pythonTuple([], false)).toBe("()");
  });
});
