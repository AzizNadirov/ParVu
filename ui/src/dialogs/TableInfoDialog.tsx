import type { Ui } from "../lib/ui";
import type { ColumnMeta, TabSummary } from "../lib/api";
import { Dialog } from "../components/Dialog";
import "./dialogs.css";

export function formatFileSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${i === 0 ? v : v.toFixed(1)} ${units[i]}`;
}

function TableInfoDialog({ ui, close, summary, columns }: { ui: Ui; close: () => void; summary: TabSummary; columns: ColumnMeta[] }) {
  const t = ui.t;
  const n = (x: number) => x.toLocaleString("en-US");
  const rows: [string, string][] = [
    [t("dialog.table_info.name"), summary.name],
    [t("dialog.table_info.file"), summary.path || "N/A"],
    [t("dialog.table_info.format"), summary.format.toUpperCase()],
    [t("dialog.table_info.file_size"), formatFileSize(summary.file_size)],
    [t("dialog.table_info.total_rows"), n(summary.total_rows)],
    [t("dialog.table_info.total_pages"), n(summary.total_pages)],
    [t("dialog.table_info.page_size"), n(summary.page_size)],
  ];
  return (
    <Dialog
      title={t("dialog.table_info")}
      onClose={close}
      className="pv-dialog-medium pv-dialog-tall"
      footer={
        <button className="primary" onClick={close}>
          {t("btn.close")}
        </button>
      }
    >
      <div className="form-grid pv-selectable">
        {rows.map(([k, v]) => (
          <div key={k} style={{ display: "contents" }}>
            <b>{k}</b>
            <span style={{ wordBreak: "break-all" }}>{v}</span>
          </div>
        ))}
      </div>
      <div className="pv-section-title">{t("dialog.table_info.columns", { count: columns.length })}</div>
      <div className="pv-table-scroll pv-fill pv-selectable">
        <table className="simple">
          <thead>
            <tr>
              <th className="num">#</th>
              <th>{t("dialog.table_info.col_name")}</th>
              <th>{t("dialog.table_info.col_type")}</th>
            </tr>
          </thead>
          <tbody>
            {columns.map((c, i) => (
              <tr key={c.name}>
                <td className="num">{i + 1}</td>
                <td>{c.name}</td>
                <td className="pv-mono">{c.dtype}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Dialog>
  );
}

export function openTableInfo(ui: Ui, a: { summary: TabSummary; columns: ColumnMeta[] }): Promise<void> {
  return ui.showDialog<void>(
    (close) => <TableInfoDialog ui={ui} close={() => close()} summary={a.summary} columns={a.columns} />,
    undefined,
  );
}
