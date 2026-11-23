import { describe, expect, it } from "vitest";
import { formatRows, markdownCell, markdownTable, pythonTuple, selectionMarkdown } from "../copy";
import golden from "../../../../crates/parvu-core/tests/golden/markdown.json";

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

// The same cases as the Rust export (crates/parvu-core/src/export.rs).
describe("markdownTable (shared golden cases)", () => {
  for (const c of golden.cases) {
    it(c.name, () => {
      const out = markdownTable(c.columns, c.rows as (string | null)[][], {
        header: c.header,
        rowNumbers: c.row_numbers,
        firstRow: c.first_row,
        numeric: c.numeric,
      });
      expect(out).toBe(c.expected);
    });
  }
});

describe("markdownCell", () => {
  it("escapes pipes and the backslashes before them, and line breaks", () => {
    expect(markdownCell("a|b")).toBe("a\\|b");
    expect(markdownCell("a\\|b")).toBe("a\\\\\\|b");
    expect(markdownCell("a\\b")).toBe("a\\b");
    expect(markdownCell("1\r\n2\r3\n")).toBe("1<br>2<br>3<br>");
    expect(markdownCell("日本 🎉")).toBe("日本 🎉");
  });
});

describe("selectionMarkdown", () => {
  const page = { offset: 200, columns: [{ dtype: "VARCHAR" }, { dtype: "BIGINT" }, { dtype: "DOUBLE" }] };
  const sel = { rows: [["7", "1.5"], [null, "2.0"]], columns: ["n", "x"] };
  const numeric = (d: string) => d === "BIGINT" || d === "DOUBLE";
  it("numbers rows absolutely: page offset + page row + 1", () => {
    const r = selectionMarkdown(page, { x: 1, y: 4, width: 2 }, sel, "row_numbers", numeric);
    expect(r.first).toBe(205);
    expect(r.last).toBe(206);
    expect(r.text).toBe(["| # | n | x |", "| ---: | ---: | ---: |", "| 205 | 7 | 1.5 |", "| 206 |  | 2.0 |"].join("\n"));
  });
  it("aligns by the selected columns' types and can leave the header blank", () => {
    const r = selectionMarkdown(page, { x: 0, y: 0, width: 2 }, sel, "no_header", numeric);
    expect(r.text.split("\n").slice(0, 2)).toEqual(["|  |  |", "| --- | ---: |"]);
    expect(selectionMarkdown(page, { x: 0, y: 0, width: 2 }, sel, "table", numeric).text.split("\n")[0]).toBe("| n | x |");
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
