import { describe, expect, it } from "vitest";
import { renderVars } from "./App";
import type { Settings } from "./lib/api";

describe("renderVars", () => {
  it("uses the tab's table name instead of the global variable name", () => {
    const s = { default_limit: "500", default_sql_font_size: 10, default_sql_font: "Courier" } as unknown as Settings;
    expect(renderVars("SELECT * \nFROM $(default_data_var_name) \nLIMIT $(default_limit)", s, "sales")).toBe(
      "SELECT * \nFROM sales \nLIMIT 500",
    );
  });
});
