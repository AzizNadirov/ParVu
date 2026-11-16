import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { Ui } from "../lib/ui";
import { api, errorMessage } from "../lib/api";
import type { McpStatus } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { Icon } from "../components/Icon";
import { copyText } from "./common";
import { mcpClientSetup } from "./logic";
import "./dialogs.css";

/** AI → Connect an AI client: lets Claude Desktop, Claude Code and other MCP
 *  clients work on this window's tables (`parvu mcp --attach`). */
function ConnectAiDialog({ ui, close }: { ui: Ui; close: () => void }) {
  const t = ui.t;
  const [status, setStatus] = useState<McpStatus | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    void api.mcpStatus().then((s) => live && setStatus(s));
    const un = listen<number>("mcp-clients", (e) => setStatus((s) => (s ? { ...s, clients: e.payload } : s)));
    return () => {
      live = false;
      void un.then((f) => f());
    };
  }, []);

  const toggle = async (on: boolean) => {
    setBusy(true);
    try {
      setStatus(await api.mcpSetEnabled(on));
    } catch (e) {
      setStatus(await api.mcpStatus());
      await ui.alert(t("dialog.connect_ai.title"), errorMessage(e), "error");
    } finally {
      setBusy(false);
    }
  };

  const setup = status ? mcpClientSetup(status.command, status.args) : null;
  const snippet = (label: string, text: string) => (
    <div className="col pv-snippet">
      <div className="row">
        <span className="grow">{label}</span>
        <button className="text small-btn" onClick={() => void copyText(ui, text, t("dialog.common.copied"))}>
          <Icon name="copy" size={16} />
          {t("dialog.connect_ai.copy")}
        </button>
      </div>
      <pre className="pv-sql-preview">{text}</pre>
    </div>
  );

  return (
    <Dialog
      title={t("dialog.connect_ai.title")}
      onClose={close}
      className="pv-dialog-wide"
      footer={
        <button className="primary" onClick={close}>
          {t("btn.close")}
        </button>
      }
    >
      <div className="pv-hint">{t("dialog.connect_ai.intro")}</div>
      <label className="row pv-switch-row">
        <input
          type="checkbox"
          disabled={!status || busy}
          checked={!!status?.enabled}
          onChange={(e) => void toggle(e.target.checked)}
        />
        <strong>{t("dialog.connect_ai.enable")}</strong>
      </label>
      <div className={status?.error ? "error-text" : "muted"}>
        {!status
          ? t("status.loading")
          : status.error
            ? t("dialog.connect_ai.error", { error: status.error })
            : status.listening
              ? t("dialog.connect_ai.listening", { count: status.clients })
              : t("dialog.connect_ai.off")}
      </div>
      {setup && (
        <>
          {snippet(t("dialog.connect_ai.claude_code"), setup.claudeCode)}
          {snippet(t("dialog.connect_ai.claude_desktop"), setup.claudeDesktop)}
          {snippet(t("dialog.connect_ai.other"), setup.plain)}
        </>
      )}
      <div className="pv-hint">{t("dialog.connect_ai.note")}</div>
    </Dialog>
  );
}

export function openConnectAi(ui: Ui): Promise<void> {
  return ui.showDialog<void>((close) => <ConnectAiDialog ui={ui} close={() => close(undefined)} />, undefined);
}
