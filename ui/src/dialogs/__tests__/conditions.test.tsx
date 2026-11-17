import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { useState } from "react";
import type { ColumnMeta, Condition } from "../../lib/api";
import { ConditionRows } from "../ConditionRows";
import { buildRowFilter, buildSetValueOp, emptyCondition, opsFor, valueKind } from "../logic";

afterEach(cleanup);

const t = (k: string) => k;
const columns: ColumnMeta[] = [
  { name: "region", dtype: "VARCHAR", family: "text" },
  { name: "qty", dtype: "INTEGER", family: "integer" },
  { name: "day", dtype: "DATE", family: "date" },
  { name: "at", dtype: "TIMESTAMP", family: "timestamp" },
  { name: "paid", dtype: "BOOLEAN", family: "boolean" },
];

function Host(props: { initial: Condition[]; suggest?: (c: string) => Promise<string[]> }) {
  const [conds, setConds] = useState(props.initial);
  return <ConditionRows t={t} columns={columns} conditions={conds} onChange={setConds} suggest={props.suggest} />;
}

describe("value kinds and operators", () => {
  it("follow the column's type", () => {
    expect(["integer", "decimal", "float", "date", "timestamp", "time", "boolean", "text", "json"].map((f) => valueKind(f as never))).toEqual([
      "number",
      "number",
      "number",
      "date",
      "datetime",
      "time",
      "boolean",
      "text",
      "text",
    ]);
    expect(valueKind(undefined, "DOUBLE")).toBe("number");
    expect(opsFor("boolean")).toEqual(["eq", "ne", "is_null", "not_null"]);
    expect(opsFor("number")).not.toContain("contains");
    expect(opsFor("text")).toContain("starts_with");
  });

  it("build row filters and Set Value", () => {
    const isText = (c: string) => c === "region";
    expect(buildRowFilter([{ ...emptyCondition("qty"), op: "gt" }], "and", isText)).toBeNull();
    const f = buildRowFilter([{ ...emptyCondition("qty"), op: "gt", value: "5" }], "or", isText)!;
    expect(f.combine).toBe("or");
    expect(buildSetValueOp("qty", "", false, f)).toBeNull();
    expect(buildSetValueOp("region", "", true, f)).toEqual({ op: "set_value", column: "region", value: "", where: f });
    expect(buildSetValueOp("qty", null, false, f)).toEqual({ op: "set_value", column: "qty", value: null, where: f });
    expect(buildSetValueOp("qty", "1", false, null)).toBeNull();
  });
});

describe("condition rows", () => {
  it("use a value box that fits the column", () => {
    const { container } = render(
      <Host
        initial={[
          { ...emptyCondition("qty"), op: "gt" },
          { ...emptyCondition("day"), op: "lt" },
          { ...emptyCondition("at"), op: "ge" },
        ]}
      />,
    );
    const types = [...container.querySelectorAll(".pv-rules input:not([type=checkbox])")].map((i) => i.getAttribute("type"));
    expect(types).toEqual(["number", "date", "datetime-local"]);
  });

  it("offer yes/no for booleans and switch operators with the column", () => {
    const { container } = render(<Host initial={[{ ...emptyCondition("region"), op: "contains" }]} />);
    const [col] = [...container.querySelectorAll(".pv-rules select")] as HTMLSelectElement[];
    fireEvent.change(col, { target: { value: "paid" } });
    const selects = [...container.querySelectorAll(".pv-rules select")] as HTMLSelectElement[];
    expect(selects[1].value).toBe("eq");
    expect([...selects[1].options].map((o) => o.value)).toEqual(["eq", "ne", "is_null", "not_null"]);
    expect([...selects[2].options].map((o) => o.value)).toEqual(["", "true", "false"]);
  });

  it("suggest a text column's values", async () => {
    const suggest = vi.fn(async () => ["north", "south"]);
    const { container } = render(<Host initial={[emptyCondition("region")]} suggest={suggest} />);
    await waitFor(() => expect(container.querySelectorAll("datalist option").length).toBe(2));
    expect(suggest).toHaveBeenCalledWith("region");
  });
});
