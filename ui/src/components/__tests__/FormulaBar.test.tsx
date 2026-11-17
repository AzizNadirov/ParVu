import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import { FormulaBar, type FormulaBarProps } from "../FormulaBar";
import { REF_COLORS_LIGHT } from "../../lib/formula";

afterEach(cleanup);

function props(over: Partial<FormulaBarProps> = {}): FormulaBarProps {
  return {
    t: (k: string) => k,
    schema: { tables: [{ name: "sales", columns: ["qty", "price"] }], functions: [], methods: [] },
    dark: false,
    editing: false,
    target: null,
    isNew: false,
    onNameChange: vi.fn(),
    value: "",
    error: "",
    onStart: vi.fn(),
    onNew: vi.fn(),
    onChange: vi.fn(),
    onApply: vi.fn(),
    onCancel: vi.fn(),
    ...over,
  };
}

describe("formula bar", () => {
  it("starts a formula when clicked while idle", () => {
    const p = props({ value: "42" });
    const { container } = render(<FormulaBar {...p} />);
    expect(container.querySelector(".cm-content")?.textContent).toBe("42");
    fireEvent.mouseDown(container.querySelector(".cm-content")!);
    expect(p.onStart).toHaveBeenCalled();
    fireEvent.click(container.querySelector("button")!);
    expect(p.onNew).toHaveBeenCalled();
  });

  it("colours references and applies with Enter", () => {
    const p = props({ editing: true, target: "total", isNew: true, value: "[qty] * [price]" });
    const { container } = render(<FormulaBar {...p} />);
    const marks = [...container.querySelectorAll(".cm-content span[style]")] as HTMLElement[];
    const colored = marks.filter((m) => m.style.color);
    expect(colored.map((m) => m.textContent)).toEqual(["[qty]", "[price]"]);
    // The first palette colour (#2a78d6), as the browser writes it.
    expect(REF_COLORS_LIGHT[0]).toBe("#2a78d6");
    expect(colored[0].style.color).toBe("rgb(42, 120, 214)");
    expect((container.querySelector(".formula-name") as HTMLInputElement).value).toBe("total");
    fireEvent.keyDown(container.querySelector(".cm-content")!, { key: "Enter" });
    expect(p.onApply).toHaveBeenCalled();
    fireEvent.keyDown(container.querySelector(".cm-content")!, { key: "Escape" });
    expect(p.onCancel).toHaveBeenCalled();
  });

  it("shows errors", () => {
    const { container } = render(<FormulaBar {...props({ editing: true, target: "x", value: "[nope]", error: "Unknown column" })} />);
    expect(container.querySelector(".formula-hint.error-text")?.textContent).toBe("Unknown column");
  });
});
