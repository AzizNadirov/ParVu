// The dialogs are loaded on first use, so their code (and the Markdown
// renderer the help dialogs need) is not parsed at start-up. Each export has
// the same signature as the eager one in ./index.
import { lazy } from "react";

type Dialogs = typeof import("./index");
type OpenFn = Exclude<keyof Dialogs, "SearchPanel">;

let loading: Promise<Dialogs> | null = null;
const load = () => (loading ??= import("./index"));

function deferred<K extends OpenFn>(name: K): Dialogs[K] {
  return ((...args: unknown[]) =>
    load().then((m) => (m[name] as (...a: unknown[]) => unknown)(...args))) as Dialogs[K];
}

/** Start loading the dialogs in the background (after start-up). */
export function preloadDialogs(): void {
  void load();
}

export const openSettingsDialog = deferred("openSettingsDialog");
export const openThemeSelector = deferred("openThemeSelector");
export const openLanguageSelector = deferred("openLanguageSelector");
export const openAboutDialog = deferred("openAboutDialog");
export const openExpressionHelp = deferred("openExpressionHelp");
export const openShortcutsDialog = deferred("openShortcutsDialog");
export const openTableInfo = deferred("openTableInfo");
export const openUniqueValues = deferred("openUniqueValues");
export const openColumnStats = deferred("openColumnStats");
export const openJoinDialog = deferred("openJoinDialog");
export const openAppendDialog = deferred("openAppendDialog");
export const openDropDuplicates = deferred("openDropDuplicates");
export const openDropNull = deferred("openDropNull");
export const openReplace = deferred("openReplace");
export const openMathDialog = deferred("openMathDialog");
export const openCopyTuple = deferred("openCopyTuple");
export const openExportDialog = deferred("openExportDialog");
export const openUnsavedChanges = deferred("openUnsavedChanges");
export const openConfirmClose = deferred("openConfirmClose");
export const openCrashReport = deferred("openCrashReport");
export const openGroupBy = deferred("openGroupBy");
export const openPivot = deferred("openPivot");
export const openMelt = deferred("openMelt");
export const openTranspose = deferred("openTranspose");
export const openValueCounts = deferred("openValueCounts");
export const openFilter = deferred("openFilter");
export const openFillNull = deferred("openFillNull");
export const openSort = deferred("openSort");
export const openColumns = deferred("openColumns");
export const openSlice = deferred("openSlice");
export const openSplitColumn = deferred("openSplitColumn");
export const openBin = deferred("openBin");
export const openWindow = deferred("openWindow");
export const openConditionalFormat = deferred("openConditionalFormat");
export const openChart = deferred("openChart");

export const SearchPanel = lazy(() => load().then((m) => ({ default: m.SearchPanel })));
