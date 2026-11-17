import { describe, expect, it } from "vitest";
import { inCode } from "../sqlComplete";

describe("SQL custom function completions", () => {
  it("are offered in code, not in strings, quoted names or comments", () => {
    expect(inCode("SELECT ")).toBe(true);
    expect(inCode("SELECT 'it''s', ")).toBe(true);
    expect(inCode("SELECT 'clean")).toBe(false);
    expect(inCode('SELECT "clean')).toBe(false);
    expect(inCode("SELECT 1 -- clean")).toBe(false);
    expect(inCode("SELECT '--', ")).toBe(true);
    expect(inCode("-- note\nSELECT ")).toBe(true);
  });
});
