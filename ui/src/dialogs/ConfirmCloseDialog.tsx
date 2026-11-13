import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import { Dialog } from "../components/Dialog";
import { qtLabel, useAutoFocus } from "./common";
import "./dialogs.css";

type Result = { result: "save_and_close" | "close" | "cancel"; dontAsk: boolean };

function ConfirmCloseDialog({ ui, close, tabs }: { ui: Ui; close: Closer<Result>; tabs: string[] }) {
  const t = ui.t;
  const [dontAsk, setDontAsk] = useState(false);
  const saveRef = useAutoFocus<HTMLButtonElement>();
  return (
    <Dialog
      title={t("dialog.confirm_close.title")}
      onClose={() => close({ result: "cancel", dontAsk: false })}
      className="pv-dialog-narrow"
      footer={
        <>
          <button onClick={() => close({ result: "cancel", dontAsk: false })}>{t("btn.cancel")}</button>
          <button className="danger" onClick={() => close({ result: "close", dontAsk })}>
            {t("dialog.confirm_close.close_anyway")}
          </button>
          <button ref={saveRef} className="primary" onClick={() => close({ result: "save_and_close", dontAsk })}>
            {qtLabel(t("dialog.confirm_close.save_and_close"))}
          </button>
        </>
      }
    >
      <div className="message-text">{t("dialog.confirm_close.message", { tabs: tabs.join(", ") })}</div>
      <label className="row">
        <input type="checkbox" checked={dontAsk} onChange={(e) => setDontAsk(e.target.checked)} />
        {t("dialog.confirm_close.dont_ask")}
      </label>
    </Dialog>
  );
}

export function openConfirmClose(ui: Ui, a: { tabs: string[] }): Promise<Result> {
  return ui.showDialog<Result>(
    (close) => <ConfirmCloseDialog ui={ui} close={close} tabs={a.tabs} />,
    { result: "cancel", dontAsk: false },
  );
}
