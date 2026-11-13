import type { Ui, Closer } from "../lib/ui";
import { Dialog } from "../components/Dialog";
import { useAutoFocus } from "./common";
import "./dialogs.css";

type Result = "save" | "discard" | "cancel";

function UnsavedChangesDialog({ ui, close, count }: { ui: Ui; close: Closer<Result>; count: number }) {
  const t = ui.t;
  const saveRef = useAutoFocus<HTMLButtonElement>();
  const msg = count === 1 ? t("warning.unsaved_changes_msg_single") : t("warning.unsaved_changes_msg", { count });
  return (
    <Dialog
      title={t("warning.unsaved_changes")}
      onClose={() => close("cancel")}
      className="pv-dialog-narrow"
      footer={
        <>
          <button className="danger" onClick={() => close("discard")}>
            {t("dialog.unsaved.discard")}
          </button>
          <button onClick={() => close("cancel")}>{t("btn.cancel")}</button>
          <button ref={saveRef} className="primary" onClick={() => close("save")}>
            {t("btn.save")}
          </button>
        </>
      }
    >
      <div className="message-text">{msg}</div>
    </Dialog>
  );
}

export function openUnsavedChanges(ui: Ui, a: { count: number }): Promise<Result> {
  return ui.showDialog<Result>((close) => <UnsavedChangesDialog ui={ui} close={close} count={a.count} />, "cancel");
}
