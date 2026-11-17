// The sheet menu of an Excel tab: switch the tab to another sheet, open
// sheets in their own tabs, and change sheets (saved with the workbook).
import type { MenuEntry } from "../components/PopupMenu";
import type { PlanSheet, SheetOp, SheetsView } from "./api";
import type { Translate } from "./i18n";

export interface SheetActions {
  /** Show this sheet (stored name) in the current tab. */
  show(sheet: PlanSheet): void;
  openInTab(sheet: PlanSheet): void;
  openAll(): void;
  rename(sheet: PlanSheet): void;
  op(op: SheetOp): void;
}

/** The menu for `view`: one entry per sheet with its actions, then the workbook's. */
export function sheetMenu(t: Translate, view: SheetsView, a: SheetActions): MenuEntry[] {
  const live = view.sheets.filter((s) => !s.deleted);
  const deleted = view.sheets.filter((s) => s.deleted);
  const visible = live.filter((s) => !s.hidden).length;
  const items: MenuEntry[] = live.map((s, i) => {
    const current = s.name === view.current;
    // A sheet not saved yet (a copy) has no data in the file to show.
    const stored = view.sheets.findIndex((x) => x.source === s.source) === view.sheets.indexOf(s);
    const openHere = view.open.includes(s.source) && stored;
    const label = s.hidden ? t("sheets.hidden_name", { name: s.name }) : s.name;
    return {
      label,
      checked: current,
      submenu: [
        { label: t("sheets.show_here"), disabled: current || !stored, onSelect: () => a.show(s) },
        { label: t("sheets.open_in_tab"), disabled: !stored, onSelect: () => a.openInTab(s) },
        { type: "separator" },
        { label: t("sheets.rename"), onSelect: () => a.rename(s) },
        s.hidden
          ? { label: t("sheets.unhide"), onSelect: () => a.op({ op: "show", sheet: s.name }) }
          : { label: t("sheets.hide"), disabled: visible < 2, onSelect: () => a.op({ op: "hide", sheet: s.name }) },
        { label: t("sheets.duplicate"), onSelect: () => a.op({ op: "duplicate", sheet: s.name }) },
        { label: t("sheets.move_left"), disabled: i === 0, onSelect: () => a.op({ op: "move", sheet: s.name, by: -1 }) },
        { label: t("sheets.move_right"), disabled: i === live.length - 1, onSelect: () => a.op({ op: "move", sheet: s.name, by: 1 }) },
        { type: "separator" },
        {
          label: openHere ? t("sheets.delete_open") : t("sheets.delete"),
          disabled: openHere || live.length < 2 || (visible < 2 && !s.hidden),
          onSelect: () => a.op({ op: "delete", sheet: s.name }),
        },
      ],
    };
  });
  if (deleted.length) {
    items.push({ type: "separator" });
    for (const s of deleted) {
      items.push({ label: t("sheets.restore", { name: s.name }), onSelect: () => a.op({ op: "restore", sheet: s.name }) });
    }
  }
  items.push({ type: "separator" });
  items.push({
    label: t("sheets.open_all"),
    disabled: view.sheets.filter((s) => !s.deleted && !view.open.includes(s.source)).length === 0,
    onSelect: a.openAll,
  });
  if (view.changed) items.push({ label: t("sheets.unsaved_hint"), disabled: true });
  return items;
}

/** Sheets that "Open all as tabs" opens: stored ones not open yet, in order. */
export function sheetsToOpen(view: SheetsView): PlanSheet[] {
  const seen = new Set<string>();
  return view.sheets.filter((s) => {
    if (s.deleted || view.open.includes(s.source) || seen.has(s.source)) return false;
    seen.add(s.source);
    return true;
  });
}
