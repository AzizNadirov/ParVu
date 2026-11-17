import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import { PopupMenu } from "../PopupMenu";

afterEach(cleanup);

describe("popup menu", () => {
  it("draws icons, and the check mark in their place when checked", () => {
    const pick = vi.fn();
    const { container } = render(
      <PopupMenu
        x={0}
        y={0}
        onClose={() => undefined}
        items={[
          { label: "Save", icon: "save", onSelect: pick },
          { label: "Steps", icon: "steps", checked: true },
          { label: "Group", icon: "clean", submenu: [{ label: "Inner" }] },
        ]}
      />,
    );
    const slots = [...container.querySelectorAll(".menu-item .menu-check")];
    expect(slots.map((s) => s.classList.contains("menu-icon"))).toEqual([true, false, true]);
    expect(slots.every((s) => s.querySelector("svg"))).toBe(true);
    expect(container.querySelectorAll(".submenu-arrow").length).toBe(1);
    fireEvent.click(container.querySelector(".menu-item")!);
    expect(pick).toHaveBeenCalled();
  });
});
