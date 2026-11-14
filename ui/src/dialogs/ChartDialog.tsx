import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import { AGG_FUNCS, api, errorMessage } from "../lib/api";
import type { AggFunc, ChartData, ChartKind, ChartSpec, ColumnMeta } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { Chart } from "../components/Chart";
import { ColumnSelect, RichText, useDebounced } from "./common";
import { defaultChartX, defaultChartY, isNumericType } from "./logic";
import "./dialogs.css";

interface Args {
  tabId: number;
  table: string;
  columns: ColumnMeta[];
  selected?: string;
}

const KINDS: ChartKind[] = ["bar", "line", "scatter", "histogram"];

/** Bar, line, scatter and histogram charts of the tab's current result. */
function ChartDialog({ ui, close, a }: { ui: Ui; close: Closer<void>; a: Args }) {
  const t = ui.t;
  const names = a.columns.map((c) => c.name);
  const numeric = a.columns.filter((c) => isNumericType(c.dtype)).map((c) => c.name);
  const selectedNumeric = a.selected && numeric.includes(a.selected);
  const [kind, setKind] = useState<ChartKind>(selectedNumeric ? "histogram" : "bar");
  const [x, setX] = useState(() => a.selected ?? defaultChartX(a.columns));
  const [y, setY] = useState(() => defaultChartY(a.columns, a.selected ?? defaultChartX(a.columns)));
  const [agg, setAgg] = useState<AggFunc>("sum");
  const [bins, setBins] = useState("20");
  const [sortByValue, setSortByValue] = useState(true);
  const [data, setData] = useState<ChartData | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const spec: ChartSpec = {
    kind,
    x,
    y: kind === "histogram" ? null : y || null,
    agg,
    bins: Number(bins) || 20,
    sort_by_value: sortByValue,
  };
  useDebounced(JSON.stringify(spec), 250, (key) => {
    let cancelled = false;
    const s = JSON.parse(key) as ChartSpec;
    if (!s.x || (s.kind === "scatter" && !s.y)) {
      setData(null);
      return;
    }
    setLoading(true);
    api
      .chartData(a.tabId, s)
      .then((d) => {
        if (cancelled) return;
        setData(d);
        setError("");
      })
      .catch((e) => !cancelled && (setData(null), setError(errorMessage(e))))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  });

  const yLabel =
    kind === "histogram" ? t("dialog.chart.rows") : kind === "scatter" ? y : y ? `${t(`agg.${agg}`)} (${y})` : t("dialog.chart.rows");

  return (
    <Dialog
      title={t("dialog.chart.title")}
      onClose={() => close(undefined)}
      className="pv-dialog-wide"
      footer={<button onClick={() => close(undefined)}>{t("btn.close")}</button>}
    >
      <div>
        <RichText text={t("dialog.common.table", { table: a.table })} />
      </div>
      <div className="row">
        {KINDS.map((k) => (
          <label key={k} className="row">
            <input type="radio" name="chart-kind" checked={kind === k} onChange={() => setKind(k)} />
            {t(`dialog.chart.${k}`)}
          </label>
        ))}
      </div>
      <div className="row">
        {t(kind === "histogram" ? "dialog.chart.values_of" : "dialog.chart.x")}
        <ColumnSelect columns={kind === "histogram" || kind === "scatter" ? numeric : names} value={x} onChange={setX} />
        {kind === "histogram" ? (
          <>
            {t("dialog.chart.bins")}
            <input type="number" min={1} max={200} className="pv-rule-num" value={bins} onChange={(e) => setBins(e.target.value)} />
          </>
        ) : (
          <>
            {t("dialog.chart.y")}
            {kind !== "scatter" && (
              <select value={agg} disabled={!y} onChange={(e) => setAgg(e.target.value as AggFunc)}>
                {AGG_FUNCS.filter((f) => f !== "rows" && f !== "list").map((f) => (
                  <option key={f} value={f}>
                    {t(`agg.${f}`)}
                  </option>
                ))}
              </select>
            )}
            <ColumnSelect
              columns={kind === "scatter" ? numeric : names}
              value={y}
              onChange={setY}
              emptyLabel={kind === "scatter" ? undefined : t("dialog.chart.count_rows")}
            />
          </>
        )}
      </div>
      {kind === "bar" && (
        <label className="row">
          <input type="checkbox" checked={sortByValue} onChange={(e) => setSortByValue(e.target.checked)} />
          {t("dialog.chart.sort_by_value")}
        </label>
      )}
      <div className="pv-chart-box">
        {error ? (
          <div className="pv-error-title">{error}</div>
        ) : data ? (
          <Chart kind={kind} data={data} xLabel={x} yLabel={yLabel} />
        ) : (
          <div className="pv-hint">{loading ? t("status.calculating") : t("dialog.chart.choose")}</div>
        )}
      </div>
      {data?.truncated && (
        <div className="pv-hint">{t(kind === "scatter" ? "dialog.chart.sampled" : kind === "bar" ? "dialog.chart.top_bars" : "dialog.chart.first_points")}</div>
      )}
    </Dialog>
  );
}

export function openChart(ui: Ui, a: Args): Promise<void> {
  return ui.showDialog<void>((close) => <ChartDialog ui={ui} close={close} a={a} />, undefined);
}
