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

  it("opens submenus from the keyboard and goes back with ArrowLeft", () => {
    const pick = vi.fn();
    const close = vi.fn();
    const { container } = render(
      <PopupMenu
        x={0}
        y={0}
        onClose={close}
        items={[
          { label: "Copy" },
          { type: "separator" },
          { label: "Copy as", submenu: [{ label: "CSV" }, { label: "Markdown", onSelect: pick }] },
        ]}
      />,
    );
    const root = container.querySelector(".menu-popup")!;
    fireEvent.keyDown(root, { key: "ArrowDown" });
    fireEvent.keyDown(root, { key: "ArrowDown" });
    fireEvent.keyDown(root, { key: "ArrowRight" });
    let menus = container.querySelectorAll(".menu-popup");
    expect(menus.length).toBe(2);
    // The submenu starts on its first item.
    expect(menus[1].querySelector(".menu-item.open")?.textContent).toBe("CSV");
    fireEvent.keyDown(menus[1], { key: "ArrowLeft" });
    expect(container.querySelectorAll(".menu-popup").length).toBe(1);
    fireEvent.keyDown(root, { key: "Enter" });
    menus = container.querySelectorAll(".menu-popup");
    fireEvent.keyDown(menus[1], { key: "ArrowDown" });
    fireEvent.keyDown(menus[1], { key: "Enter" });
    expect(pick).toHaveBeenCalled();
    expect(close).toHaveBeenCalled();
  });
});
