import { describe, expect, it } from "vitest";
import { continueSeries, fillEdge, fillFromHandle } from "../fill";
import type { Cell } from "../api";

describe("continueSeries", () => {
  it("numbers continue the step; one number is copied", () => {
    expect(continueSeries(["1", "2"], 3)).toEqual(["3", "4", "5"]);
    expect(continueSeries(["5", "10"], 2)).toEqual(["15", "20"]);
    expect(continueSeries(["10", "8"], 3)).toEqual(["6", "4", "2"]);
    expect(continueSeries(["7"], 2)).toEqual(["7", "7"]);
    expect(continueSeries(["1", "2", "4"], 1)).toEqual(["5.5"]);
  });

  it("numbers keep their decimals without float noise", () => {
    expect(continueSeries(["0.1", "0.2"], 2)).toEqual(["0.3", "0.4"]);
    expect(continueSeries(["1.50", "1.75"], 2)).toEqual(["2.00", "2.25"]);
    expect(continueSeries(["-1", "-0.5"], 2)).toEqual(["0", "0.5"]);
  });

  it("dates go by day, or by month and year when the day of the month is shared", () => {
    expect(continueSeries(["2024-01-30"], 3)).toEqual(["2024-01-31", "2024-02-01", "2024-02-02"]);
    expect(continueSeries(["2024-01-01", "2024-01-08"], 2)).toEqual(["2024-01-15", "2024-01-22"]);
    expect(continueSeries(["2024-01-31", "2024-02-29"], 1)).toEqual(["2024-03-31"]);
    expect(continueSeries(["2024-11-15", "2024-12-15"], 2)).toEqual(["2025-01-15", "2025-02-15"]);
    expect(continueSeries(["2020-02-29", "2021-02-28"], 1)).toEqual(["2022-02-28"]);
    expect(continueSeries(["2023-01-31", "2023-02-28"], 2)).toEqual(["2023-03-31", "2023-04-30"]);
    expect(continueSeries(["2023-01-10", "2023-02-28"], 1)).toEqual(["2023-04-18"]);
  });

  it("month steps clamp to the month's length", () => {
    // Landing on 29 February of a leap year (the drag fill had rolled it to 1 March).
    expect(continueSeries(["2024-04-29", "2024-03-29"], 1)).toEqual(["2024-02-29"]);
    expect(continueSeries(["2023-12-31", "2024-01-31"], 1)).toEqual(["2024-02-29"]);
    expect(continueSeries(["2024-01-31", "2024-03-31"], 1)).toEqual(["2024-05-31"]);
    expect(continueSeries(["2024-10-31", "2024-12-31"], 1)).toEqual(["2025-02-28"]);
  });

  it("timestamps keep their format", () => {
    expect(continueSeries(["2024-01-01 10:00:00", "2024-01-01 10:30:00"], 2)).toEqual([
      "2024-01-01 11:00:00",
      "2024-01-01 11:30:00",
    ]);
    expect(continueSeries(["2024-01-01T23:00"], 1)).toEqual(["2024-01-02T23:00"]);
    expect(continueSeries(["2024-01-01 00:00:00.500", "2024-01-01 00:00:01.000"], 1)).toEqual(["2024-01-01 00:00:01.500"]);
  });

  it("month and day names, keeping their case", () => {
    expect(continueSeries(["Jan"], 2)).toEqual(["Feb", "Mar"]);
    expect(continueSeries(["November", "December"], 2)).toEqual(["January", "February"]);
    expect(continueSeries(["MON", "WED"], 2)).toEqual(["FRI", "SUN"]);
    expect(continueSeries(["sunday"], 1)).toEqual(["monday"]);
    expect(continueSeries(["Mar", "Feb"], 2)).toEqual(["Jan", "Dec"]);
    expect(continueSeries(["Январь"], 2)).toEqual(["Февраль", "Март"]);
    expect(continueSeries(["пн", "вт"], 1)).toEqual(["ср"]);
    expect(continueSeries(["Yanvar"], 1)).toEqual(["Fevral"]);
    expect(continueSeries(["bazar ertəsi"], 1)).toEqual(["çərşənbə axşamı"]);
  });

  it("text ending in a number", () => {
    expect(continueSeries(["Item 1"], 2)).toEqual(["Item 2", "Item 3"]);
    expect(continueSeries(["Q1", "Q3"], 1)).toEqual(["Q5"]);
    expect(continueSeries(["INV-007"], 3)).toEqual(["INV-008", "INV-009", "INV-010"]);
    expect(continueSeries(["007"], 1)).toEqual(["008"]);
    // Different texts: repeat.
    expect(continueSeries(["A1", "B2"], 3)).toEqual(["A1", "B2", "A1"]);
    // Would go below zero: repeat.
    expect(continueSeries(["x2", "x1"], 3)).toEqual(["x2", "x1", "x2"]);
  });

  it("anything else repeats the seeds", () => {
    expect(continueSeries(["a", "b"], 5)).toEqual(["a", "b", "a", "b", "a"]);
    expect(continueSeries(["1", null], 3)).toEqual(["1", null, "1"]);
    expect(continueSeries(["1", "x"], 2)).toEqual(["1", "x"]);
    expect(continueSeries(["2024-01-01", "2024-01-02", "2024-01-05"], 1)).toEqual(["2024-01-01"]);
    expect(continueSeries([], 3)).toEqual([]);
  });
});

const grid: Cell[][] = [
  ["1", "a", "Jan"],
  ["2", "b", "Feb"],
  [null, null, null],
  [null, null, null],
];
const get = (r: number, c: number) => grid[r]?.[c] ?? null;

describe("fillFromHandle", () => {
  it("drags down: each column continues its own series", () => {
    const cells = fillFromHandle(get, { x: 0, y: 0, width: 3, height: 2 }, { x: 0, y: 0, width: 3, height: 4 });
    expect(cells).toEqual([
      { row: 2, col: 0, value: "3" },
      { row: 3, col: 0, value: "4" },
      { row: 2, col: 1, value: "a" },
      { row: 3, col: 1, value: "b" },
      { row: 2, col: 2, value: "Mar" },
      { row: 3, col: 2, value: "Apr" },
    ]);
  });

  it("drags up: the series runs backward", () => {
    const g: Cell[][] = [[null], [null], ["3"], ["4"]];
    const cells = fillFromHandle((r, c) => g[r][c], { x: 0, y: 2, width: 1, height: 2 }, { x: 0, y: 0, width: 1, height: 4 });
    expect(cells).toEqual([
      { row: 1, col: 0, value: "2" },
      { row: 0, col: 0, value: "1" },
    ]);
  });

  it("drags sideways along rows", () => {
    const g: Cell[][] = [["Mon", null, null]];
    const cells = fillFromHandle((r, c) => g[r][c], { x: 0, y: 0, width: 1, height: 1 }, { x: 0, y: 0, width: 3, height: 1 });
    expect(cells.map((c) => c.value)).toEqual(["Tue", "Wed"]);
    const left = fillFromHandle((r, c) => [["", "1", "2"]][r][c], { x: 1, y: 0, width: 2, height: 1 }, { x: 0, y: 0, width: 3, height: 1 });
    expect(left).toEqual([{ row: 0, col: 0, value: "0" }]);
  });
});

describe("fillEdge", () => {
  const g: Cell[][] = [
    ["a", "b"],
    ["c", "d"],
    ["e", "f"],
  ];
  const at = (r: number, c: number) => g[r][c];
  const all = { x: 0, y: 0, width: 2, height: 3 };

  it("down copies the first row, up the last", () => {
    expect(fillEdge(at, all, "down").map((c) => c.value)).toEqual(["a", "b", "a", "b"]);
    expect(fillEdge(at, all, "up").map((c) => `${c.row}${c.col}${c.value}`)).toEqual(["00e", "01f", "10e", "11f"]);
  });

  it("right copies the first column, left the last", () => {
    expect(fillEdge(at, all, "right").map((c) => c.value)).toEqual(["a", "c", "e"]);
    expect(fillEdge(at, all, "left").map((c) => `${c.row}${c.col}${c.value}`)).toEqual(["00b", "10d", "20f"]);
  });

  it("needs two rows or columns", () => {
    expect(fillEdge(at, { x: 0, y: 0, width: 2, height: 1 }, "down")).toEqual([]);
    expect(fillEdge(at, { x: 0, y: 0, width: 1, height: 3 }, "right")).toEqual([]);
  });
});

describe("series with an explicit step", () => {
  const spec = (s: Partial<import("../fill").SeriesSpec>): import("../fill").SeriesSpec => ({
    direction: "down", type: "linear", unit: "day", step: 1, stop: "", ...s,
  });

  it("linear and growth, with a stop value", async () => {
    const { seriesValues } = await import("../fill");
    expect(seriesValues("1", 4, spec({ step: 2 }))).toEqual(["3", "5", "7", "9"]);
    expect(seriesValues("1", 10, spec({ step: 2, stop: "6" }))).toEqual(["3", "5"]);
    expect(seriesValues("10", 10, spec({ step: -3, stop: "2" }))).toEqual(["7", "4"]);
    expect(seriesValues("0.5", 3, spec({ step: 0.25 }))).toEqual(["0.75", "1.00", "1.25"]);
    expect(seriesValues("2", 4, spec({ type: "growth", step: 3 }))).toEqual(["6", "18", "54", "162"]);
    expect(seriesValues("2", 10, spec({ type: "growth", step: 2, stop: "20" }))).toEqual(["4", "8", "16"]);
    expect(seriesValues("100", 2, spec({ type: "growth", step: 0.5 }))).toEqual(["50", "25"]);
    expect(seriesValues("x", 2, spec({}))).toBeNull();
    expect(seriesValues(null, 2, spec({}))).toBeNull();
    expect(seriesValues("1", 2, spec({ stop: "soon" }))).toBeNull();
  });

  it("dates by day, week, month and year", async () => {
    const { seriesValues } = await import("../fill");
    const d = (s: Partial<import("../fill").SeriesSpec>) => spec({ type: "date", ...s });
    expect(seriesValues("2024-01-30", 3, d({}))).toEqual(["2024-01-31", "2024-02-01", "2024-02-02"]);
    expect(seriesValues("2024-01-01", 2, d({ unit: "week", step: 2 }))).toEqual(["2024-01-15", "2024-01-29"]);
    expect(seriesValues("2024-01-31", 2, d({ unit: "month" }))).toEqual(["2024-02-29", "2024-03-31"]);
    expect(seriesValues("2024-02-29", 1, d({ unit: "year" }))).toEqual(["2025-02-28"]);
    expect(seriesValues("2024-01-01 08:00:00", 1, d({}))).toEqual(["2024-01-02 08:00:00"]);
    expect(seriesValues("2024-01-01", 10, d({ stop: "2024-01-03" }))).toEqual(["2024-01-02", "2024-01-03"]);
    expect(seriesValues("2024-01-01", 1, d({ step: 1.5 }))).toBeNull();
    expect(seriesValues("Jan", 1, d({}))).toBeNull();
  });

  it("fills each column (or row) from its first cell", async () => {
    const { fillSeries } = await import("../fill");
    const g: (string | null)[][] = [["1", "10"], [null, null], [null, null]];
    const cells = fillSeries((r, c) => g[r][c], { x: 0, y: 0, width: 2, height: 3 }, spec({ step: 5 }));
    expect(cells).toEqual([
      { row: 1, col: 0, value: "6" },
      { row: 2, col: 0, value: "11" },
      { row: 1, col: 1, value: "15" },
      { row: 2, col: 1, value: "20" },
    ]);
    const right = fillSeries((r, c) => [["2024-01-01", null, null]][r][c], { x: 0, y: 0, width: 3, height: 1 }, spec({ direction: "right", type: "date", unit: "month" }));
    expect(right.map((c) => c.value)).toEqual(["2024-02-01", "2024-03-01"]);
  });
});
