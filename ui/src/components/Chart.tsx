// A small SVG chart (bar, line, scatter, histogram) drawn in the theme's
// colours. The data arrives aggregated from DuckDB, so this only lays out
// at most a few thousand marks.
import type { ChartData, ChartKind } from "../lib/api";
import { formatTick, niceTicks, shorten } from "../lib/chart";

const W = 720;
const H = 380;
const M = { top: 12, right: 16, bottom: 70, left: 64 };

export function Chart({ kind, data, xLabel, yLabel }: { kind: ChartKind; data: ChartData; xLabel: string; yLabel: string }) {
  const plotW = W - M.left - M.right;
  const plotH = H - M.top - M.bottom;
  const ys = data.values.filter((v): v is number => v !== null && Number.isFinite(v));
  if (!ys.length) return <div className="pv-hint">—</div>;
  const yMin = kind === "bar" || kind === "histogram" ? Math.min(0, ...ys) : Math.min(...ys);
  const yMax = Math.max(...ys, kind === "bar" || kind === "histogram" ? 0 : -Infinity);
  const yTicks = niceTicks(yMin, yMax);
  const y0 = yTicks[0];
  const y1 = yTicks[yTicks.length - 1];
  const sy = (v: number) => M.top + plotH - ((v - y0) / (y1 - y0 || 1)) * plotH;

  let marks: React.ReactNode = null;
  let xAxis: React.ReactNode = null;

  if (kind === "scatter") {
    const xsAll = data.xs.filter((v): v is number => v !== null && Number.isFinite(v));
    const xTicks = niceTicks(Math.min(...xsAll), Math.max(...xsAll));
    const x0 = xTicks[0];
    const x1 = xTicks[xTicks.length - 1];
    const sx = (v: number) => M.left + ((v - x0) / (x1 - x0 || 1)) * plotW;
    marks = data.xs.map((x, i) => {
      const y = data.values[i];
      if (x === null || y === null) return null;
      return <circle key={i} cx={sx(x)} cy={sy(y)} r={2.5} className="pv-chart-mark pv-chart-dot" />;
    });
    xAxis = xTicks.map((tv) => (
      <g key={tv}>
        <line x1={sx(tv)} x2={sx(tv)} y1={M.top} y2={M.top + plotH} className="pv-chart-grid" />
        <text x={sx(tv)} y={M.top + plotH + 16} textAnchor="middle" className="pv-chart-text">
          {formatTick(tv)}
        </text>
      </g>
    ));
  } else {
    const n = data.labels.length;
    const band = plotW / Math.max(1, n);
    const cx = (i: number) => M.left + band * i + band / 2;
    // Show at most ~14 x labels, evenly spaced.
    const every = Math.max(1, Math.ceil(n / 14));
    xAxis = data.labels.map((l, i) =>
      i % every === 0 ? (
        <text
          key={i}
          x={kind === "histogram" ? M.left + band * i : cx(i)}
          y={M.top + plotH + 14}
          textAnchor="end"
          transform={`rotate(-35 ${kind === "histogram" ? M.left + band * i : cx(i)} ${M.top + plotH + 14})`}
          className="pv-chart-text"
        >
          {shorten(l, 16)}
        </text>
      ) : null,
    );
    if (kind === "line") {
      const pts = data.values
        .map((v, i) => (v === null ? null : `${cx(i)},${sy(v)}`))
        .filter(Boolean)
        .join(" ");
      marks = (
        <>
          <polyline points={pts} className="pv-chart-line" />
          {n <= 60 &&
            data.values.map((v, i) =>
              v === null ? null : <circle key={i} cx={cx(i)} cy={sy(v)} r={3} className="pv-chart-mark pv-chart-dot" />,
            )}
        </>
      );
    } else {
      const gap = kind === "histogram" ? 1 : Math.min(8, band * 0.2);
      const base = sy(Math.max(y0, Math.min(0, y1)));
      marks = data.values.map((v, i) => {
        if (v === null) return null;
        const top = Math.min(sy(v), base);
        return (
          <rect key={i} x={M.left + band * i + gap / 2} y={top} width={Math.max(1, band - gap)} height={Math.abs(base - sy(v))} className="pv-chart-mark">
            <title>{`${data.labels[i]}: ${v}`}</title>
          </rect>
        );
      });
    }
  }

  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="pv-chart" role="img" aria-label={`${yLabel} / ${xLabel}`}>
      {yTicks.map((tv) => (
        <g key={tv}>
          <line x1={M.left} x2={M.left + plotW} y1={sy(tv)} y2={sy(tv)} className="pv-chart-grid" />
          <text x={M.left - 6} y={sy(tv) + 4} textAnchor="end" className="pv-chart-text">
            {formatTick(tv)}
          </text>
        </g>
      ))}
      {xAxis}
      <line x1={M.left} x2={M.left} y1={M.top} y2={M.top + plotH} className="pv-chart-axis" />
      <line x1={M.left} x2={M.left + plotW} y1={M.top + plotH} y2={M.top + plotH} className="pv-chart-axis" />
      {marks}
      <text x={M.left + plotW / 2} y={H - 4} textAnchor="middle" className="pv-chart-text pv-chart-title">
        {xLabel}
      </text>
      <text x={14} y={M.top + plotH / 2} textAnchor="middle" transform={`rotate(-90 14 ${M.top + plotH / 2})`} className="pv-chart-text pv-chart-title">
        {yLabel}
      </text>
    </svg>
  );
}
