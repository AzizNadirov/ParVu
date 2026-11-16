import { useMemo, useRef, useState } from "react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import type { Ui, Closer } from "../lib/ui";
import { AGG_FUNCS, api, errorMessage } from "../lib/api";
import type { AggFunc, ChartData, ChartKind, ChartSort, ChartSpec, ColumnMeta, TimeUnit } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { Chart, pngBytes, svgMarkup, type Ink } from "../components/Chart";
import { Icon } from "../components/Icon";
import { chartTitle, columnRole, formatValue, isDark, suggestChart, switchKind } from "../lib/chart";
import { ColumnSelect, useDebounced } from "./common";
import { isNumericType } from "./logic";
import "./dialogs.css";

interface Args {
  tabId: number;
  table: string;
  columns: ColumnMeta[];
  selected?: string;
}

const KINDS: ChartKind[] = ["bar", "line", "area", "scatter", "histogram"];
const UNITS: TimeUnit[] = ["auto", "day", "week", "month", "quarter", "year", "none"];
const SORTS: ChartSort[] = ["auto", "value_desc", "value_asc", "label"];
const LIMITS = [5, 10, 20, 50, 100];

/** Theme colours resolved to plain values (exported images can't use CSS variables). */
function resolveInk(): Ink {
  const probe = (css: string, fallback: string) => {
    try {
      const el = document.createElement("span");
      el.style.color = css;
      document.body.appendChild(el);
      const c = getComputedStyle(el).color;
      el.remove();
      return c || fallback;
    } catch {
      return fallback;
    }
  };
  let font = "sans-serif";
  try {
    font = getComputedStyle(document.body).fontFamily || font;
  } catch {
    /* tests */
  }
  return {
    surface: probe("var(--pv-colors-table-background)", "#ffffff"),
    text: probe("var(--pv-colors-table-foreground)", "#1d1b20"),
    muted: probe("var(--md-on-surface-variant)", "#5d5b60"),
    grid: probe("var(--md-outline-variant)", "#e3e1e6"),
    axis: probe("var(--md-outline)", "#8e8c92"),
    font,
  };
}

export function initialSpec(a: Args): ChartSpec {
  const s = suggestChart(a.columns, a.selected);
  return {
    kind: s.kind,
    x: s.x,
    y: s.y ?? null,
    agg: (s.agg as AggFunc | undefined) ?? "sum",
    series: null,
    time_unit: "auto",
    sort: "auto",
    limit: 20,
    other: true,
    bins: 20,
  };
}

/** Bar, line, area, scatter and histogram charts of the tab's current result. */
function ChartDialog({ ui, close, a }: { ui: Ui; close: Closer<void>; a: Args }) {
  const t = ui.t;
  const names = a.columns.map((c) => c.name);
  const numeric = a.columns.filter((c) => isNumericType(c.dtype)).map((c) => c.name);
  const splittable = a.columns.filter((c) => columnRole(c) === "category").map((c) => c.name);
  const [spec, setSpec] = useState<ChartSpec>(() => initialSpec(a));
  const [title, setTitle] = useState("");
  // Unset until the user picks: split bars and areas start stacked (grouped
  // bars grow tall fast, overlapping area washes turn muddy).
  const [stackedChoice, setStacked] = useState<boolean | null>(null);
  const stacked = stackedChoice ?? true;
  const [showValues, setShowValues] = useState(true);
  const [table, setTable] = useState(false);
  const [data, setData] = useState<ChartData | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const svg = useRef<SVGSVGElement>(null);
  const ink = useMemo(resolveInk, []);
  const dark = isDark(ink.surface);
  const set = (patch: Partial<ChartSpec>) => setSpec((s) => ({ ...s, ...patch }));

  const xCol = a.columns.find((c) => c.name === spec.x);
  const xRole = xCol ? columnRole(xCol) : "category";
  const isHist = spec.kind === "histogram";
  const isScatter = spec.kind === "scatter";
  const ready = !!spec.x && (!isScatter || !!spec.y);
  const request: ChartSpec = {
    ...spec,
    y: isHist ? null : spec.y || null,
    series: isHist ? null : spec.series || null,
  };

  useDebounced(JSON.stringify(request), 250, (key) => {
    let cancelled = false;
    const s = JSON.parse(key) as ChartSpec;
    if (!ready) {
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

  const autoTitle = chartTitle(request, t);
  const shownTitle = title.trim() || autoTitle;
  const yLabel = isHist
    ? t("dialog.chart.rows")
    : isScatter
      ? spec.y ?? ""
      : spec.y
        ? t("chart.title.measure", { agg: t(`agg.${spec.agg}`), y: spec.y })
        : t("dialog.chart.rows");
  const multi = (data?.series.length ?? 0) > 1;

  const saveImage = async () => {
    if (!svg.current) return;
    const base = shownTitle.replace(/[\\/:*?"<>|]+/g, " ").trim().slice(0, 80) || "chart";
    const path = await saveDialog({
      defaultPath: `${base}.png`,
      filters: [
        { name: "PNG", extensions: ["png"] },
        { name: "SVG", extensions: ["svg"] },
      ],
    });
    if (!path) return;
    try {
      const bytes = path.toLowerCase().endsWith(".svg") ? new TextEncoder().encode(svgMarkup(svg.current)) : await pngBytes(svg.current);
      await api.saveChartImage(path, bytes);
      ui.status(t("dialog.chart.saved", { path }), 4000);
    } catch (e) {
      await ui.alert(t("dialog.chart.title"), errorMessage(e), "error");
    }
  };

  return (
    <Dialog
      title={t("dialog.chart.title")}
      onClose={() => close(undefined)}
      className="pv-dialog-chart"
      footer={
        <>
          <div className="left">
            <label className="row">
              <input type="checkbox" checked={table} onChange={(e) => setTable(e.target.checked)} />
              {t("dialog.chart.table_view")}
            </label>
          </div>
          <button className="outlined" disabled={!data} onClick={() => void saveImage()}>
            <Icon name="download" size={16} />
            {t("dialog.chart.save_image")}
          </button>
          <button className="primary" onClick={() => close(undefined)}>
            {t("btn.close")}
          </button>
        </>
      }
    >
      <div className="pv-chart-controls">
        <div className="segmented" role="radiogroup" aria-label={t("dialog.chart.kind")}>
          {KINDS.map((k) => (
            <button key={k} role="radio" aria-checked={spec.kind === k} className={spec.kind === k ? "active" : ""} onClick={() => setSpec((s) => switchKind(s, k, a.columns))}>
              {spec.kind === k && <Icon name="check" size={16} />}
              {t(`dialog.chart.${k}`)}
            </button>
          ))}
        </div>
        <button className="text small-btn" title={t("dialog.chart.suggest_hint")} onClick={() => setSpec(initialSpec(a))}>
          {t("dialog.chart.suggest")}
        </button>
      </div>
      <div className="pv-chart-form">
        <label>{t(isHist ? "dialog.chart.values_of" : isScatter ? "dialog.chart.x" : "dialog.chart.group_by")}</label>
        <ColumnSelect columns={isHist || isScatter ? numeric : names} value={spec.x} onChange={(x) => set({ x })} />
        {!isHist && (
          <>
            <label>{t(isScatter ? "dialog.chart.y" : "dialog.chart.value")}</label>
            <div className="row">
              {!isScatter && (
                <select value={spec.y ? spec.agg : "rows"} disabled={!spec.y} onChange={(e) => set({ agg: e.target.value as AggFunc })} aria-label={t("dialog.chart.calculation")}>
                  {AGG_FUNCS.filter((f) => f !== "rows" && f !== "list").map((f) => (
                    <option key={f} value={f}>
                      {t(`agg.${f}`)}
                    </option>
                  ))}
                </select>
              )}
              <ColumnSelect
                className="grow"
                columns={isScatter ? numeric : names}
                value={spec.y ?? ""}
                onChange={(y) => set({ y: y || null })}
                emptyLabel={isScatter ? undefined : t("dialog.chart.count_rows")}
              />
            </div>
            <label>{t("dialog.chart.split")}</label>
            <ColumnSelect
              columns={splittable.filter((c) => c !== spec.x)}
              value={spec.series ?? ""}
              onChange={(series) => set({ series: series || null })}
              emptyLabel={t("dialog.chart.no_split")}
            />
          </>
        )}
        {isHist && (
          <>
            <label>{t("dialog.chart.bins")}</label>
            <input type="number" min={1} max={200} className="pv-rule-num" value={spec.bins ?? 20} onChange={(e) => set({ bins: Number(e.target.value) || 20 })} />
          </>
        )}
        {!isHist && !isScatter && xRole === "time" && (
          <>
            <label>{t("dialog.chart.group_dates")}</label>
            <select value={spec.time_unit} onChange={(e) => set({ time_unit: e.target.value as TimeUnit })}>
              {UNITS.map((u) => (
                <option key={u} value={u}>
                  {u === "auto" && data?.time_unit && data.time_unit !== "auto"
                    ? t("dialog.chart.unit_auto_is", { unit: t(`dialog.chart.unit.${data.time_unit}`) })
                    : t(`dialog.chart.unit.${u}`)}
                </option>
              ))}
            </select>
          </>
        )}
        {!isHist && !isScatter && xRole === "category" && (
          <>
            <label>{t("dialog.chart.show")}</label>
            <div className="row">
              <select value={spec.limit ?? 20} onChange={(e) => set({ limit: Number(e.target.value) })} aria-label={t("dialog.chart.show")}>
                {LIMITS.map((n) => (
                  <option key={n} value={n}>
                    {t("dialog.chart.top_n", { n })}
                  </option>
                ))}
              </select>
              <label className="row">
                <input type="checkbox" checked={spec.other} onChange={(e) => set({ other: e.target.checked })} />
                {t("dialog.chart.group_rest")}
              </label>
              <select value={spec.sort} onChange={(e) => set({ sort: e.target.value as ChartSort })} aria-label={t("dialog.chart.sort")}>
                {SORTS.map((s) => (
                  <option key={s} value={s}>
                    {t(`dialog.chart.sort.${s}`)}
                  </option>
                ))}
              </select>
            </div>
          </>
        )}
        <label>{t("dialog.chart.chart_title")}</label>
        <input type="text" value={title} placeholder={autoTitle} onChange={(e) => setTitle(e.target.value)} />
        {!isScatter && !isHist && (
          <>
            <span />
            <div className="row">
              {multi && spec.kind !== "line" && (
                <label className="row">
                  <input type="checkbox" checked={stacked} onChange={(e) => setStacked(e.target.checked)} />
                  {t("dialog.chart.stacked")}
                </label>
              )}
              {spec.kind === "bar" && (
                <label className="row">
                  <input type="checkbox" checked={showValues} onChange={(e) => setShowValues(e.target.checked)} />
                  {t("dialog.chart.show_values")}
                </label>
              )}
            </div>
          </>
        )}
      </div>
      <div className={`pv-chart-box${loading && data ? " refreshing" : ""}`}>
        {error ? (
          <div className="pv-error-title">{error}</div>
        ) : data ? (
          <Chart
            kind={spec.kind}
            data={data}
            title={shownTitle}
            xLabel={spec.x}
            yLabel={yLabel}
            stacked={stacked}
            showValues={showValues}
            ink={ink}
            dark={dark}
            svgRef={svg}
          />
        ) : (
          <div className="pv-hint">{loading ? t("status.calculating") : t("dialog.chart.choose")}</div>
        )}
      </div>
      {data && <ChartNotes ui={ui} data={data} kind={spec.kind} />}
      {data && table && <ChartTable data={data} kind={spec.kind} xLabel={spec.x} yLabel={yLabel} ui={ui} />}
    </Dialog>
  );
}

function ChartNotes({ ui, data, kind }: { ui: Ui; data: ChartData; kind: ChartKind }) {
  const t = ui.t;
  const notes: string[] = [];
  if (data.truncated) notes.push(t(kind === "scatter" ? "dialog.chart.sampled" : "dialog.chart.first_points"));
  if (data.other_label !== null) notes.push(t("dialog.chart.other_note"));
  if (data.series.some((s) => s.other)) notes.push(t("dialog.chart.other_series_note"));
  return notes.length ? <div className="pv-hint">{notes.join(" ")}</div> : null;
}

/** The chart's numbers as a table (the accessible view of every value). */
function ChartTable({ data, kind, xLabel, yLabel, ui }: { data: ChartData; kind: ChartKind; xLabel: string; yLabel: string; ui: Ui }) {
  if (kind === "scatter") {
    const rows = data.series.flatMap((s) => s.values.map((v, i) => [s.name, s.xs[i], v] as const)).slice(0, 500);
    const split = data.series.some((s) => s.name);
    return (
      <div className="pv-table-scroll pv-chart-table">
        <table className="simple">
          <thead>
            <tr>
              {split && <th />}
              <th>{xLabel}</th>
              <th>{yLabel}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map(([name, x, y], i) => (
              <tr key={i}>
                {split && <td>{name}</td>}
                <td className="num">{formatValue(x)}</td>
                <td className="num">{formatValue(y)}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {rows.length === 500 && <div className="pv-hint">{ui.t("dialog.chart.table_first", { n: 500 })}</div>}
      </div>
    );
  }
  return (
    <div className="pv-table-scroll pv-chart-table">
      <table className="simple">
        <thead>
          <tr>
            <th>{kind === "histogram" ? xLabel : xLabel}</th>
            {data.series.map((s, i) => (
              <th key={i} className="num">
                {s.name || yLabel}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {data.labels.map((l, i) => (
            <tr key={i}>
              <td>{l}</td>
              {data.series.map((s, j) => (
                <td key={j} className="num">
                  {formatValue(s.values[i])}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function openChart(ui: Ui, a: Args): Promise<void> {
  return ui.showDialog<void>((close) => <ChartDialog ui={ui} close={close} a={a} />, undefined);
}
