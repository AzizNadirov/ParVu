// Series…: fill the selected cells from the first value with an explicit
// step (Excel's Fill → Series), for when dragging the fill handle is awkward.
import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { Cell } from "../lib/api";
import { isDateValue, seriesValues, type SeriesSpec } from "../lib/fill";
import { Dialog } from "../components/Dialog";
import { SubmitOnEnter } from "./common";
import "./dialogs.css";

interface Args {
  /** The first selected cell: where every column (or row) starts. */
  seed: Cell;
  rows: number;
  cols: number;
}

function SeriesDialog({ ui, close, a }: { ui: Ui; close: Closer<SeriesSpec | null>; a: Args }) {
  const t = ui.t;
  const [direction, setDirection] = useState<SeriesSpec["direction"]>(a.cols > a.rows ? "right" : "down");
  const [type, setType] = useState<SeriesSpec["type"]>(isDateValue(a.seed) ? "date" : "linear");
  const [unit, setUnit] = useState<SeriesSpec["unit"]>("day");
  const [step, setStep] = useState("1");
  const [stop, setStop] = useState("");

  const stepValue = Number(step);
  const spec: SeriesSpec = { direction, type, unit, step: stepValue, stop };
  const sample = step.trim() !== "" && Number.isFinite(stepValue) ? seriesValues(a.seed, 4, spec) : null;
  const valid = sample !== null;
  const submit = () => valid && close(spec);

  const radio = <T extends string>(name: string, value: T, current: T, set: (v: T) => void, text: string, disabled = false) => (
    <label className={`row${disabled ? " muted" : ""}`}>
      <input type="radio" name={name} checked={current === value} disabled={disabled} onChange={() => set(value)} />
      {text}
    </label>
  );

  return (
    <Dialog
      title={t("dialog.series.title")}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-narrow"
      footer={
        <>
          <button onClick={() => close(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={!valid} onClick={submit}>
            {t("btn.ok")}
          </button>
        </>
      }
    >
      <SubmitOnEnter onSubmit={submit}>
        <div className="muted">{t("dialog.series.from", { value: a.seed ?? "NULL" })}</div>
        <fieldset className="col">
          <legend>{t("dialog.series.in")}</legend>
          {radio("pv-series-dir", "down", direction, setDirection, t("dialog.series.in_columns", { count: a.rows - 1 }), a.rows < 2)}
          {radio("pv-series-dir", "right", direction, setDirection, t("dialog.series.in_rows", { count: a.cols - 1 }), a.cols < 2)}
        </fieldset>
        <fieldset className="col">
          <legend>{t("dialog.series.type")}</legend>
          {radio("pv-series-type", "linear", type, setType, t("dialog.series.type_linear"))}
          {radio("pv-series-type", "growth", type, setType, t("dialog.series.type_growth"))}
          {radio("pv-series-type", "date", type, setType, t("dialog.series.type_date"))}
          {type === "date" && (
            <label className="row pv-indent">
              {t("dialog.series.unit")}
              <select value={unit} onChange={(e) => setUnit(e.target.value as SeriesSpec["unit"])}>
                {(["day", "week", "month", "year"] as const).map((u) => (
                  <option key={u} value={u}>
                    {t(`dialog.series.unit_${u}`)}
                  </option>
                ))}
              </select>
            </label>
          )}
        </fieldset>
        <div className="form-grid">
          <label htmlFor="pv-series-step">{t("dialog.series.step")}</label>
          <input id="pv-series-step" type="number" step="any" value={step} onChange={(e) => setStep(e.target.value)} />
          <label htmlFor="pv-series-stop">{t("dialog.series.stop")}</label>
          <input
            id="pv-series-stop"
            type="text"
            value={stop}
            placeholder={type === "date" ? "2025-12-31" : "100"}
            onChange={(e) => setStop(e.target.value)}
          />
        </div>
        {valid ? (
          <div className="pv-hint">{t("dialog.series.preview", { values: [a.seed, ...sample].join(", ") + " …" })}</div>
        ) : (
          <div className="error-text">{t(type === "date" ? "dialog.series.bad_date" : "dialog.series.bad_number")}</div>
        )}
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openSeries(ui: Ui, a: Args): Promise<SeriesSpec | null> {
  return ui.showDialog<SeriesSpec | null>((close) => <SeriesDialog ui={ui} close={close} a={a} />, null);
}
