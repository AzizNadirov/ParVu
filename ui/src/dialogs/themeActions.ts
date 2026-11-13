// Import / export / delete of themes, shared by the theme selector and settings.
import { open, save } from "@tauri-apps/plugin-dialog";
import type { Ui } from "../lib/ui";
import { api, errorMessage } from "../lib/api";
import { themeFileName } from "./logic";

const filters = (ui: Ui) => [
  { name: ui.t("dialog.theme.filter"), extensions: ["json"] },
  { name: ui.t("dialog.common.all_files"), extensions: ["*"] },
];

/** Resolves with the imported theme's name, or null when cancelled / failed. */
export async function importThemeFlow(ui: Ui): Promise<string | null> {
  const t = ui.t;
  const path = await open({ title: t("dialog.import_theme"), multiple: false, directory: false, filters: filters(ui) });
  if (!path || Array.isArray(path)) return null;
  try {
    const name = await api.importTheme(path);
    await ui.alert(t("success.import_theme"), t("success.import_theme_msg", { name }));
    return name;
  } catch (e) {
    await ui.alert(t("success.import_failed"), `${t("success.import_failed_msg")}\n\n${errorMessage(e)}`, "error");
    return null;
  }
}

export async function exportThemeFlow(ui: Ui, name: string | null): Promise<void> {
  const t = ui.t;
  if (!name) {
    await ui.alert(t("theme.selector.no_selection"), t("theme.selector.no_selection_msg"), "warning");
    return;
  }
  const path = await save({ title: t("dialog.export_theme"), defaultPath: themeFileName(name), filters: filters(ui) });
  if (!path) return;
  try {
    await api.exportTheme(name, path);
    await ui.alert(t("success.export_theme"), t("success.export_theme_msg", { path }));
  } catch (e) {
    await ui.alert(t("success.export_failed"), errorMessage(e), "error");
  }
}

/** Resolves true when the theme was deleted. */
export async function deleteThemeFlow(ui: Ui, name: string): Promise<boolean> {
  const t = ui.t;
  if (!(await ui.confirm(t("theme.selector.delete_confirm"), t("theme.selector.delete_confirm_msg", { name })))) {
    return false;
  }
  try {
    await api.deleteTheme(name);
    ui.status(t("success.theme_deleted_msg", { name }), 4000);
    return true;
  } catch (e) {
    await ui.alert(t("theme.selector.delete_failed"), `${t("theme.selector.delete_failed_msg")}\n\n${errorMessage(e)}`, "warning");
    return false;
  }
}
