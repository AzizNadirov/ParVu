import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { ColumnMeta, Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { RichText } from "./common";
import { buildSelectColumnsOp, moveItem } from "./logic";
import "./dialogs.css";

interface Args {
  table: string;
  columns: ColumnMeta[];
}

/** Choose which columns to keep and in which order (pandas df[[...]]). */
function ColumnsDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const [order, setOrder] = useState(() => a.columns.map((c) => c.name));
  const [kept, setKept] = useState<Set<string>>(() => new Set(order));
  const [current, setCurrent] = useState(0);
  const op = buildSelectColumnsOp(order, kept);
  const unchanged = kept.size === order.length && order.every((c, i) => c === a.columns[i]?.name);
  const submit = () => op && !unchanged && close(op);
  const move = (delta: number) => {
    setOrder((l) => moveItem(l, current, delta));
    setCurrent((c) => Math.max(0, Math.min(order.length - 1, c + delta)));
  };
  const toggle = (c: string) =>
    setKept((s) => {
      const n = new Set(s);
      if (n.has(c)) n.delete(c);
      else n.add(c);
      return n;
    });

  return (
    <Dialog
      title={t("dialog.columns.title")}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-medium pv-dialog-tall"
      footer={
        <>
          <button onClick={() => close(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={!op || unchanged} onClick={submit}>
            {t("btn.apply")}
          </button>
        </>
      }
    >
      <div>
        <RichText text={t("dialog.common.table", { table: a.table })} />
      </div>
      <div className="row">
        <button onClick={() => setKept(new Set(order))}>{t("btn.select_all")}</button>
        <button onClick={() => setKept(new Set())}>{t("btn.deselect_all")}</button>
        <span className="grow" />
        <button onClick={() => move(-1)} disabled={current === 0}>
          ▲ {t("dialog.columns.up")}
        </button>
        <button onClick={() => move(1)} disabled={current >= order.length - 1}>
          ▼ {t("dialog.columns.down")}
        </button>
      </div>
      <div className="list-box pv-list-fill">
        {order.map((c, i) => (
          <div
            key={c}
            className={`item ${i === current ? "selected" : ""}`}
            onClick={() => setCurrent(i)}
            onKeyDown={(e) => {
              if (e.altKey && e.key === "ArrowUp") move(-1);
              if (e.altKey && e.key === "ArrowDown") move(1);
            }}
          >
            <label className="row grow">
              <input type="checkbox" checked={kept.has(c)} onChange={() => toggle(c)} />
              {c}
            </label>
          </div>
        ))}
      </div>
      <div className="pv-hint">{t("dialog.columns.hint", { kept: kept.size, total: order.length })}</div>
    </Dialog>
  );
}

export function openColumns(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <ColumnsDialog ui={ui} close={close} a={a} />, null);
}
