import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { BinMethod, ColumnMeta, Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { ColumnSelect, RichText, SubmitOnEnter } from "./common";
import { buildBinOp, isNumericType } from "./logic";
import "./dialogs.css";

interface Args {
  table: string;
  columns: ColumnMeta[];
  selected?: string;
}

/** pandas cut / qcut. */
function BinDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const numeric = a.columns.filter((c) => isNumericType(c.dtype)).map((c) => c.name);
  const [column, setColumn] = useState(a.selected && numeric.includes(a.selected) ? a.selected : numeric[0] ?? "");
  const [method, setMethod] = useState<BinMethod>("equal_width");
  const [bins, setBins] = useState("5");
  const [edges, setEdges] = useState("");
  const [labels, setLabels] = useState("");
  const [output, setOutput] = useState<string | null>(null);
  const out = output ?? (column ? `${column}_bin` : "");
  const op = buildBinOp(column, method, Number(bins), edges, labels, out);
  const submit = () => op && close(op);

  return (
    <Dialog
      title={t("dialog.bin.title")}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-medium"
      footer={
        <>
          <button onClick={() => close(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={!op} onClick={submit}>
            {t("btn.apply")}
          </button>
        </>
      }
    >
      <SubmitOnEnter onSubmit={submit}>
        <div>
          <RichText text={t("dialog.common.table", { table: a.table })} />
        </div>
        {numeric.length === 0 && <div className="pv-hint">{t("dialog.bin.no_numeric")}</div>}
        <label className="row">
          {t("dialog.common.column")}
          <ColumnSelect columns={numeric} value={column} onChange={setColumn} />
        </label>
        {(["equal_width", "quantile", "edges"] as const).map((m) => (
          <label key={m} className="row">
            <input type="radio" name="bin-method" checked={method === m} onChange={() => setMethod(m)} />
            {t(`dialog.bin.${m}`)}
          </label>
        ))}
        {method === "edges" ? (
          <label className="row">
            {t("dialog.bin.edges")}
            <input
              type="text"
              className="grow pv-mono"
              placeholder="0, 18, 65, 120"
              value={edges}
              onChange={(e) => setEdges(e.target.value)}
            />
          </label>
        ) : (
          <label className="row">
            {t("dialog.bin.bins")}
            <input type="number" min={1} max={1000} className="grow" value={bins} onChange={(e) => setBins(e.target.value)} />
          </label>
        )}
        <label className="row">
          {t("dialog.bin.labels")}
          <input
            type="text"
            className="grow"
            placeholder={t("dialog.bin.labels_placeholder")}
            value={labels}
            onChange={(e) => setLabels(e.target.value)}
          />
        </label>
        <label className="row">
          {t("dialog.common.output")}
          <input type="text" className="grow" value={out} onChange={(e) => setOutput(e.target.value)} />
        </label>
        <div className="pv-hint">{t("dialog.bin.hint")}</div>
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openBin(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <BinDialog ui={ui} close={close} a={a} />, null);
}
