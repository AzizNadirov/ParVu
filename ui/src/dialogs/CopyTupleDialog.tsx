import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import { Dialog } from "../components/Dialog";
import { RichText, SubmitOnEnter } from "./common";
import { clampInt, initialSampleSize } from "./logic";
import "./dialogs.css";

type Mode = "page" | "first" | "random";
type Result = { mode: Mode; n: number; label: string } | null;

interface Args {
  column: string;
  totalRows: number;
  pageSize: number;
  defaultSample: number;
}

function CopyTupleDialog({ ui, close, a }: { ui: Ui; close: Closer<Result>; a: Args }) {
  const t = ui.t;
  const [mode, setMode] = useState<Mode>("page");
  const [n, setN] = useState(String(initialSampleSize(a.defaultSample, a.totalRows)));
  const [label, setLabel] = useState("");
  const nValue = parseInt(n, 10);
  const valid = mode === "page" || (Number.isFinite(nValue) && nValue >= 1 && nValue <= 1000);
  const submit = () => {
    if (!valid) return;
    close({ mode, n: mode === "page" ? a.pageSize : clampInt(nValue, 1, 1000), label: label.trim() });
  };
  const fmt = (x: number) => x.toLocaleString("en-US");
  const radio = (m: Mode, text: string) => (
    <label className="row">
      <input type="radio" name="pv-copy-mode" checked={mode === m} onChange={() => setMode(m)} />
      {text}
    </label>
  );
  return (
    <Dialog
      title={t("dialog.copy_tuple.title")}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-narrow"
      footer={
        <>
          <button onClick={() => close(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={!valid} onClick={submit}>
            {t("dialog.copy_tuple.copy")}
          </button>
        </>
      }
    >
      <SubmitOnEnter onSubmit={submit}>
        <div>
          <RichText text={t("dialog.copy_tuple.column", { column: a.column })} />
          <br />
          {t("dialog.copy_tuple.total_rows", { total_rows: fmt(a.totalRows) })}
        </div>
        <fieldset className="col">
          <legend>{t("dialog.copy_tuple.what")}</legend>
          {radio("page", t("dialog.copy_tuple.page_only", { page_size: fmt(a.pageSize) }))}
          {radio("first", t("dialog.copy_tuple.first_n"))}
          {radio("random", t("dialog.copy_tuple.random_n"))}
          <label className="row pv-indent">
            N:
            <input
              type="number"
              min={1}
              max={1000}
              value={n}
              disabled={mode === "page"}
              onChange={(e) => setN(e.target.value)}
            />
          </label>
        </fieldset>
        <label className="row">
          {t("dialog.copy_tuple.label")}
          <input
            type="text"
            className="grow"
            placeholder={t("dialog.copy_tuple.label_placeholder")}
            value={label}
            onChange={(e) => setLabel(e.target.value)}
          />
        </label>
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openCopyTuple(ui: Ui, a: Args): Promise<Result> {
  return ui.showDialog<Result>((close) => <CopyTupleDialog ui={ui} close={close} a={a} />, null);
}
