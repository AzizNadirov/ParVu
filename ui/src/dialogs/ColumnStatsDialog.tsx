import type { Ui } from "../lib/ui";
import type { ColumnStats } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { RichText, copyText } from "./common";
import { statsRows } from "./logic";
import "./dialogs.css";

function ColumnStatsDialog({ ui, close, stats }: { ui: Ui; close: () => void; stats: ColumnStats }) {
  const t = ui.t;
  const rows = statsRows(stats).map(([k, v]) => [t(k), v] as const);
  const copy = () =>
    copyText(ui, rows.map(([k, v]) => `${k}\t${v}`).join("\n"), t("dialog.common.copied"));
  return (
    <Dialog
      title={t("column_stats.title", { column: stats.column })}
      onClose={close}
      className="pv-dialog-narrow"
      footer={
        <>
          <button onClick={copy}>{t("column_stats.copy")}</button>
          <button className="primary" onClick={close}>
            {t("btn.close")}
          </button>
        </>
      }
    >
      <div>
        <RichText text={t("column_stats.header", { column: stats.column, type: stats.dtype || "?" })} />
      </div>
      <table className="simple pv-selectable">
        <thead>
          <tr>
            <th>{t("column_stats.col.stat")}</th>
            <th>{t("column_stats.col.value")}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(([k, v]) => (
            <tr key={k}>
              <td className="nowrap">{k}</td>
              <td className="num">{v}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </Dialog>
  );
}

export function openColumnStats(ui: Ui, a: { stats: ColumnStats }): Promise<void> {
  return ui.showDialog<void>((close) => <ColumnStatsDialog ui={ui} close={() => close()} stats={a.stats} />, undefined);
}
