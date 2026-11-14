import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { Operation, SliceMode } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { RichText, SubmitOnEnter } from "./common";
import { buildSliceOp } from "./logic";
import "./dialogs.css";

interface Args {
  table: string;
  rows: number;
}

function SliceDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const [mode, setMode] = useState<SliceMode>("head");
  const [n, setN] = useState("100");
  const [seed, setSeed] = useState("");
  const op = buildSliceOp(mode, Number(n), seed);
  const submit = () => op && close(op);

  return (
    <Dialog
      title={t("dialog.slice.title")}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-narrow"
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
          <RichText text={t("dialog.slice.table", { table: a.table, rows: a.rows })} />
        </div>
        {(["head", "tail", "sample"] as const).map((m) => (
          <label key={m} className="row">
            <input type="radio" name="slice-mode" checked={mode === m} onChange={() => setMode(m)} />
            {t(`dialog.slice.${m}`)}
          </label>
        ))}
        <label className="row">
          {t("dialog.slice.rows")}
          <input type="number" min={1} className="grow" value={n} onChange={(e) => setN(e.target.value)} />
        </label>
        {mode === "sample" && (
          <label className="row">
            {t("dialog.slice.seed")}
            <input
              type="text"
              className="grow"
              placeholder={t("dialog.slice.seed_placeholder")}
              value={seed}
              onChange={(e) => setSeed(e.target.value)}
            />
          </label>
        )}
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openSlice(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <SliceDialog ui={ui} close={close} a={a} />, null);
}
