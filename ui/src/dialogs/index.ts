// All dialogs of the app. Each `openXxx(ui, args)` shows a modal through
// `ui.showDialog` and resolves with its result.
export { openSettingsDialog } from "./SettingsDialog";
export { openThemeSelector } from "./ThemeSelector";
export { openLanguageSelector } from "./LanguageSelector";
export { openAboutDialog } from "./AboutDialog";
export { openExpressionHelp } from "./ExpressionHelpDialog";
export { openShortcutsDialog } from "./ShortcutsDialog";
export { openTableInfo } from "./TableInfoDialog";
export { openUniqueValues } from "./UniqueValuesDialog";
export { openColumnStats } from "./ColumnStatsDialog";
export { openJoinDialog } from "./JoinDialog";
export { openAppendDialog } from "./AppendDialog";
export { openDropDuplicates } from "./DropDuplicatesDialog";
export { openDropNull } from "./DropNullDialog";
export { openReplace } from "./ReplaceDialog";
export { openMathDialog } from "./MathDialog";
export { openCopyTuple } from "./CopyTupleDialog";
export { openExportDialog } from "./ExportDialog";
export { openUnsavedChanges } from "./UnsavedChangesDialog";
export { openConfirmClose } from "./ConfirmCloseDialog";
export { openCrashReport } from "./CrashReportDialog";
export { SearchPanel } from "./SearchPanel";
export type { SearchPanelProps } from "./SearchPanel";
