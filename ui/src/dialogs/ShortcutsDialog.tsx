import type { Ui } from "../lib/ui";
import type { Translate } from "../lib/i18n";
import { Dialog } from "../components/Dialog";
import { RichText } from "./common";
import "./dialogs.css";

/** Translation-key rows; a value that is not a key (e.g. "Ctrl+N") is shown as-is. */
export const SHORTCUT_GROUPS: [string, [string, string][]][] = [
  [
    "shortcuts.group.file",
    [
      ["shortcuts.action.new_window", "Ctrl+N"],
      ["shortcuts.action.open_file", "Ctrl+O"],
      ["shortcuts.action.save", "Ctrl+S"],
      ["shortcuts.action.save_as", "Ctrl+Shift+S"],
      ["shortcuts.action.drag_drop", "shortcuts.value.drag_drop"],
    ],
  ],
  [
    "shortcuts.group.table",
    [
      ["shortcuts.action.sort", "shortcuts.value.sort"],
      ["shortcuts.action.column_actions", "shortcuts.value.column_actions"],
      ["shortcuts.action.cell_actions", "shortcuts.value.cell_actions"],
      ["shortcuts.action.copy_tsv", "Ctrl+C"],
      ["shortcuts.action.copy_other", "shortcuts.value.copy_other"],
      ["shortcuts.action.hscroll", "shortcuts.value.hscroll"],
      ["shortcuts.action.filter", "Ctrl+Shift+L"],
      ["shortcuts.action.group_by", "Ctrl+G"],
      ["shortcuts.action.fill_down", "Ctrl+D"],
      ["shortcuts.action.chart", "Ctrl+Shift+C"],
    ],
  ],
  [
    "shortcuts.group.find",
    [
      ["shortcuts.action.find", "Ctrl+F"],
      ["shortcuts.action.find_next", "shortcuts.value.find_next"],
      ["shortcuts.action.find_prev", "shortcuts.value.find_prev"],
      ["shortcuts.action.find_close", "Esc"],
    ],
  ],
  ["shortcuts.group.help", [["shortcuts.action.cheatsheet", "Ctrl+/"]]],
];

export function maybeTranslate(t: Translate, value: string): string {
  return value.includes(".") && !/^(Ctrl|Shift|Alt|F\d)/.test(value) ? t(value) : value;
}

export function shortcutRows(t: Translate): [string, string, string][] {
  return SHORTCUT_GROUPS.flatMap(([g, items]) =>
    items.map(([a, k]) => [t(g), t(a), maybeTranslate(t, k)] as [string, string, string]),
  );
}

function ShortcutsDialog({ ui, close }: { ui: Ui; close: () => void }) {
  const t = ui.t;
  const rows = shortcutRows(t);
  return (
    <Dialog
      title={t("shortcuts.title")}
      onClose={close}
      className="pv-dialog-medium"
      footer={
        <button className="primary" onClick={close}>
          {t("btn.close")}
        </button>
      }
    >
      <div>
        <RichText text={t("shortcuts.intro")} />
      </div>
      <div className="pv-table-scroll">
        <table className="simple">
          <thead>
            <tr>
              <th>{t("shortcuts.col.group")}</th>
              <th>{t("shortcuts.col.action")}</th>
              <th>{t("shortcuts.col.shortcut")}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map(([g, a, k], i) => (
              <tr key={i}>
                <td className="nowrap">{g}</td>
                <td>{a}</td>
                <td className="nowrap">
                  <kbd className="pv-mono">{k}</kbd>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Dialog>
  );
}

export function openShortcutsDialog(ui: Ui): Promise<void> {
  return ui.showDialog<void>((close) => <ShortcutsDialog ui={ui} close={() => close()} />, undefined);
}
