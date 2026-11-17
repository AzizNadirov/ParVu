import type { Ui, Closer } from "../lib/ui";
import type { McpProposal } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { stepText } from "../components/Panels";
import { RichText } from "./common";
import "./dialogs.css";

/** An AI client asks to change a table; resolves true when the user approves. */
function McpProposalDialog({ ui, close, p }: { ui: Ui; close: Closer<boolean>; p: McpProposal }) {
  const t = ui.t;
  const undo = p.kind === "undo";
  const step = stepText(t, p.label);
  return (
    <Dialog
      title={t(undo ? "mcp.proposal.undo_title" : "mcp.proposal.title", { client: p.client, table: p.table })}
      onClose={() => close(false)}
      className="pv-dialog-wide"
      footer={
        <>
          <button onClick={() => close(false)}>{t("mcp.proposal.reject")}</button>
          <button className="primary" onClick={() => close(true)}>
            {t(undo ? "mcp.proposal.undo" : "mcp.proposal.apply")}
          </button>
        </>
      }
    >
      <div className="pv-callout pv-proposal-step">
        <strong>{step}</strong>
        {p.rationale && <div className="muted">{t("mcp.proposal.why", { rationale: p.rationale })}</div>}
        {p.kind === "new_table" && <div className="muted">{t("mcp.proposal.new_table", { table: p.table })}</div>}
      </div>
      <div>
        <RichText
          text={
            p.rows_after === null
              ? t("mcp.proposal.rows_now", { before: p.rows_before })
              : t("mcp.proposal.rows", { before: p.rows_before, after: p.rows_after })
          }
        />
      </div>
      {p.columns.length > 0 && (
        <>
          <div className="pv-hint">{t("mcp.proposal.preview")}</div>
          <div className="pv-table-scroll pv-proposal-preview">
            <table className="simple">
              <thead>
                <tr>
                  {p.columns.map((c) => (
                    <th key={c.name}>{c.name}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {p.rows.map((r, i) => (
                  <tr key={i}>
                    {r.map((v, j) => (
                      <td key={j} className={v === null ? "pv-null-value" : undefined}>
                        {v ?? "NULL"}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
      {p.sql && (
        <details>
          <summary>{t("mcp.proposal.sql")}</summary>
          <pre className="pv-sql-preview">{p.sql}</pre>
        </details>
      )}
    </Dialog>
  );
}

export function openMcpProposal(ui: Ui, p: McpProposal): Promise<boolean> {
  return ui.showDialog<boolean>((close) => <McpProposalDialog ui={ui} close={close} p={p} />, false);
}
