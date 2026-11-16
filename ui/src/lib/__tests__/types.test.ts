import { describe, expect, it } from "vitest";
import { dayFirst, headerIconName, headerIcons, readAsChoices, sameReading, swapDayMonth } from "../types";
import en from "../../../../crates/parvu-config/locales/en.json";

describe("column types", () => {
  it("swaps day and month", () => {
    expect(swapDayMonth("%d/%m/%Y")).toBe("%m/%d/%Y");
    expect(swapDayMonth("%m/%d/%Y %H:%M")).toBe("%d/%m/%Y %H:%M");
    expect(swapDayMonth("%Y-%m")).toBe("%Y-%m");
    expect(dayFirst("%d.%m.%Y")).toBe(true);
    expect(dayFirst("%m/%d/%Y")).toBe(false);
  });

  it("names a header icon for every family, and a question mark for guessed dates", () => {
    const icons = headerIcons();
    expect(headerIconName({ name: "a", dtype: "BIGINT", family: "integer" })).toBe("pv-integer");
    expect(headerIconName({ name: "a", dtype: "X" })).toBe("pv-other");
    const guess = { name: "d", reading: { kind: "date_format" as const, format: "%d/%m/%Y" }, native: false, ambiguous: true };
    expect(headerIconName({ name: "d", dtype: "DATE", family: "date" }, guess)).toBe("pv-ambiguous");
    for (const name of ["pv-integer", "pv-json", "pv-ambiguous"]) {
      const svg = icons[name]({ fgColor: "#123456", bgColor: "#fff" });
      expect(svg).toContain('fill="#123456"');
      expect(svg.startsWith("<svg")).toBe(true);
    }
  });

  it("offers distinct readings, each with a translation", () => {
    const choices = readAsChoices();
    const seen = new Set(choices.map((c) => JSON.stringify(c.reading)));
    expect(seen.size).toBe(choices.length);
    for (const c of choices) expect((en as Record<string, string>)[c.key], c.key).toBeTruthy();
    expect(sameReading({ kind: "date_format", format: "%d/%m/%Y" }, choices[9].reading)).toBe(true);
  });
});
