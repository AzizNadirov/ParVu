import { describe, expect, it } from "vitest";
import { format, makeTranslator } from "../i18n";

describe("format", () => {
  it("substitutes names", () => {
    expect(format("Page {page} of {total_pages}", { page: 2, total_pages: 10 })).toBe("Page 2 of 10");
  });
  it("supports thousands separators like Python {:,}", () => {
    expect(format("{rows:,} rows", { rows: 1234567 })).toBe("1,234,567 rows");
    expect(format("{x:,}", { x: 12 })).toBe("12");
  });
  it("supports fixed precision like Python {:.1f}", () => {
    expect(format("{size:.1f} MB", { size: 12.345 })).toBe("12.3 MB");
    expect(format("{size:.2f}", { size: 2 })).toBe("2.00");
  });
  it("keeps escaped braces and unknown placeholders", () => {
    expect(format("{{literal}} {missing}", { other: 1 })).toBe("{literal} {missing}");
  });
  it("returns the template unchanged without args", () => {
    expect(format("a {b}")).toBe("a {b}");
  });
  it("renders null as None like Python", () => {
    expect(format("{v}", { v: null })).toBe("None");
  });
});

describe("makeTranslator", () => {
  const t = makeTranslator({ a: "Привет {name}" }, { a: "Hello {name}", b: "Only English" });
  it("uses the current language", () => expect(t("a", { name: "X" })).toBe("Привет X"));
  it("falls back to English", () => expect(t("b")).toBe("Only English"));
  it("falls back to the key", () => expect(t("missing.key")).toBe("missing.key"));
});
