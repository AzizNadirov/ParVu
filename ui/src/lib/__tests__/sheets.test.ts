import { describe, expect, it, vi } from "vitest";
import { sheetMenu, sheetsToOpen } from "../sheets";
import type { PlanSheet, SheetsView } from "../api";
import type { MenuEntry } from "../../components/PopupMenu";

const t = (k: string, a?: Record<string, unknown>) => (a ? `${k}:${Object.values(a).join(",")}` : k);
const sheet = (name: string, extra: Partial<PlanSheet> = {}): PlanSheet => ({ name, source: name, hidden: false, deleted: false, ...extra });
const view = (sheets: PlanSheet[], extra: Partial<SheetsView> = {}): SheetsView => ({ current: sheets[0].name, sheets, open: [sheets[0].source], changed: false, ...extra });

type Item = Extract<MenuEntry, { label: string }>;
const items = (m: MenuEntry[]) => m.filter((e): e is Item => e.type !== "separator");
const sub = (m: MenuEntry[], label: string) => items(items(m).find((e) => e.label === label)!.submenu!);
const entry = (m: MenuEntry[], label: string) => items(m).find((e) => e.label === label)!;

describe("sheet menu", () => {
  const actions = () => ({ show: vi.fn(), openInTab: vi.fn(), openAll: vi.fn(), rename: vi.fn(), op: vi.fn() });

  it("lists sheets, marks the current and hidden ones", () => {
    const m = sheetMenu(t as never, view([sheet("Q1"), sheet("Q2", { hidden: true }), sheet("Totals")]), actions());
    const labels = items(m).map((e) => e.label);
    expect(labels.slice(0, 3)).toEqual(["Q1", "sheets.hidden_name:Q2", "Totals"]);
    expect(entry(m, "Q1").checked).toBe(true);
    expect(entry(m, "sheets.hidden_name:Q2").submenu!.some((e) => e.type !== "separator" && e.label === "sheets.unhide")).toBe(true);
  });

  it("guards what can't be done", () => {
    const m = sheetMenu(t as never, view([sheet("Q1"), sheet("Q2")]), actions());
    // The open sheet can't be deleted, nor shown again here; the first can't move left.
    expect(entry(sub(m, "Q1"), "sheets.delete_open").disabled).toBe(true);
    expect(entry(sub(m, "Q1"), "sheets.show_here").disabled).toBe(true);
    expect(entry(sub(m, "Q1"), "sheets.move_left").disabled).toBe(true);
    expect(entry(sub(m, "Q2"), "sheets.delete").disabled).toBe(false);
    expect(entry(sub(m, "Q2"), "sheets.move_right").disabled).toBe(true);
    // One visible sheet left: it can't be hidden.
    const one = sheetMenu(t as never, view([sheet("A"), sheet("B", { hidden: true })]), actions());
    expect(entry(sub(one, "A"), "sheets.hide").disabled).toBe(true);
  });

  it("an unsaved copy can be renamed or deleted but not shown yet", () => {
    const v = view([sheet("Q1"), sheet("Q1 (2)", { source: "Q1" })]);
    const m = sheetMenu(t as never, v, actions());
    expect(entry(sub(m, "Q1 (2)"), "sheets.show_here").disabled).toBe(true);
    expect(entry(sub(m, "Q1 (2)"), "sheets.delete").disabled).toBe(false);
  });

  it("deleted sheets can be restored, and actions call back", () => {
    const a = actions();
    const m = sheetMenu(t as never, view([sheet("Q1"), sheet("Old", { deleted: true })], { changed: true }), a);
    entry(m, "sheets.restore:Old").onSelect!();
    expect(a.op).toHaveBeenCalledWith({ op: "restore", sheet: "Old" });
    entry(sub(m, "Q1"), "sheets.duplicate").onSelect!();
    expect(a.op).toHaveBeenCalledWith({ op: "duplicate", sheet: "Q1" });
    expect(entry(m, "sheets.unsaved_hint").disabled).toBe(true);
  });

  it("open all opens stored sheets not open yet, once each", () => {
    const v = view([sheet("Q1"), sheet("Q2"), sheet("Q2 (2)", { source: "Q2" }), sheet("X", { deleted: true })]);
    expect(sheetsToOpen(v).map((s) => s.name)).toEqual(["Q2"]);
  });
});
