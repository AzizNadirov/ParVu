import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import type { Ui } from "../lib/ui";
import { errorMessage } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { copyText, useAutoFocus } from "./common";
import { mailtoUrl } from "./logic";
import "./dialogs.css";

function CrashReportDialog({ ui, close, report }: { ui: Ui; close: () => void; report: string }) {
  const t = ui.t;
  const email = ui.settings.bug_report_email || "";
  const logDir = ui.boot.log_dir;
  const closeRef = useAutoFocus<HTMLButtonElement>();

  const copy = async () => {
    if (await copyText(ui, report)) {
      await ui.alert(t("dialog.crash.copied_title"), t("dialog.crash.copied_msg", { email }));
    }
  };
  const sendMail = async () => {
    const body = t("dialog.crash.mail_body", { log_dir: logDir, report: report.slice(0, 2000) });
    try {
      await openUrl(mailtoUrl(email, t("dialog.crash.mail_subject"), body));
    } catch (e) {
      await ui.alert(t("dialog.crash.error_title"), t("dialog.crash.mail_failed", { error: errorMessage(e), email }), "warning");
    }
  };
  const openLogs = async () => {
    try {
      await openPath(logDir);
    } catch (e) {
      await ui.alert(t("dialog.crash.error_title"), t("dialog.crash.logs_failed", { error: errorMessage(e), path: logDir }), "warning");
    }
  };

  return (
    <Dialog
      title={t("dialog.crash.title")}
      onClose={close}
      className="pv-dialog-wide pv-dialog-tall"
      footer={
        <>
          <div className="left">
            <button onClick={copy}>{t("dialog.crash.copy")}</button>
            <button onClick={sendMail} disabled={!email}>
              {t("dialog.crash.send_email")}
            </button>
            <button onClick={openLogs} disabled={!logDir}>
              {t("dialog.crash.open_logs")}
            </button>
          </div>
          <button ref={closeRef} className="primary" onClick={close}>
            {t("btn.close")}
          </button>
        </>
      }
    >
      <div className="pv-error-title">⚠ {t("dialog.crash.heading")}</div>
      <div>{t("dialog.crash.description")}</div>
      <div className="pv-section-title">{t("dialog.crash.details")}</div>
      <textarea className="pv-report" readOnly value={report} spellCheck={false} />
      <div className="pv-callout pv-selectable">
        <div>
          <b>{t("dialog.crash.send_to")}</b> {email}
        </div>
        <div className="pv-hint">{t("dialog.crash.instructions", { log_dir: logDir })}</div>
      </div>
    </Dialog>
  );
}

export function openCrashReport(ui: Ui, a: { report: string }): Promise<void> {
  return ui.showDialog<void>((close) => <CrashReportDialog ui={ui} close={() => close()} report={a.report} />, undefined);
}
