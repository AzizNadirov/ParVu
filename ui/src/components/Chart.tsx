// Charts drawn as SVG from data DuckDB already aggregated. The marks follow
// one spec: bars at most 24px thick with a 4px rounded data end (square at
// the baseline) and a 2px surface gap between touching bars, 2px lines,
// 8px dots with a 2px surface ring, a 10% area wash, solid hairline grids.
// Colours are resolved values (not CSS variables), so an exported SVG or
// PNG looks exactly like the screen.
import { useEffect, useMemo, useRef, useState } from "react";
import type { ChartData, ChartKind } from "../lib/api";
import { axisFormatter, compactValue, formatValue, niceTicks, seriesColors, shorten, valueExtent } from "../lib/chart";

/** Resolved theme colours and font for the chart. */
export interface Ink {
  surface: string;
  text: string;
  muted: string;
  grid: string;
  axis: string;
  font: string;
}

export interface ChartProps {
  kind: ChartKind;
  data: ChartData;
  title: string;
  /** Name of the x axis (the category column, or the value for histograms). */
  xLabel: string;
  /** Name of the measured value. */
  yLabel: string;
  stacked: boolean;
  showValues: boolean;
  ink: Ink;
  dark: boolean;
  svgRef?: React.Ref<SVGSVGElement>;
}

interface TipRow {
  color: string;
  key: "line" | "rect" | "dot";
  name: string;
  value: string;
}
interface Tip {
  x: number;
  y: number;
  title: string;
  rows: TipRow[];
}

const FONT = 12;
const CHAR = 6.6; // average glyph width at 12px
const RADIUS = 4;

/** A bar from `base` to `end` along its axis, rounded only at the data end. */
function barPath(horizontal: boolean, base: number, end: number, across: number, thick: number, round: boolean): string {
  const len = Math.abs(end - base);
  if (len < 0.5) return "";
  const r = round ? Math.min(RADIUS, len, thick / 2) : 0;
  const dir = end >= base ? 1 : -1;
  if (horizontal) {
    const y0 = across;
    const y1 = across + thick;
    const e = end;
    return `M${base},${y0} L${e - dir * r},${y0} Q${e},${y0} ${e},${y0 + r} L${e},${y1 - r} Q${e},${y1} ${e - dir * r},${y1} L${base},${y1} Z`;
  }
  const x0 = across;
  const x1 = across + thick;
  const e = end;
  // dir is in screen space: data grows upward, so end < base for positives.
  const up = end < base ? 1 : -1;
  return `M${x0},${base} L${x0},${e + up * r} Q${x0},${e} ${x0 + r},${e} L${x1 - r},${e} Q${x1},${e} ${x1},${e + up * r} L${x1},${base} Z`;
}

function useWidth(ref: React.RefObject<HTMLDivElement | null>, fallback = 720): number {
  const [w, setW] = useState(fallback);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => setW(Math.max(420, Math.floor(el.clientWidth) || fallback));
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref, fallback]);
  return w;
}

export function Chart(props: ChartProps) {
  const { kind, data, ink } = props;
  const box = useRef<HTMLDivElement>(null);
  const width = useWidth(box);
  const [tip, setTip] = useState<Tip | null>(null);
  const colors = useMemo(() => seriesColors(data.series, props.dark), [data.series, props.dark]);
  const multi = data.series.length > 1;
  const hasData = data.series.some((s) => s.values.some((v) => v !== null && Number.isFinite(v)));

  const legend = multi ? (
    <Legend series={data.series.map((s, i) => ({ name: s.name, color: colors[i] }))} lineKeys={kind === "line"} ink={ink} width={width} />
  ) : null;
  const legendHeight = legend ? legendRows(data.series.map((s) => s.name), width) * 20 + 6 : 0;
  const top = 34 + legendHeight;

  let body: { height: number; marks: React.ReactNode; hit: (x: number, y: number) => Tip | null; cursor?: (x: number | null) => React.ReactNode };
  if (!hasData) {
    body = { height: 120, marks: null, hit: () => null };
  } else if (kind === "scatter") {
    body = scatterBody(props, colors, width, top);
  } else if (kind === "bar" && data.x_kind === "category") {
    body = horizontalBody(props, colors, width, top);
  } else {
    body = verticalBody(props, colors, width, top);
  }
  const [hoverX, setHoverX] = useState<number | null>(null);
  const height = top + body.height;

  const onMove = (e: React.MouseEvent<SVGSVGElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const x = ((e.clientX - r.left) / r.width) * width;
    const y = ((e.clientY - r.top) / r.height) * height;
    const t = body.hit(x, y);
    setTip(t);
    setHoverX(t ? x : null);
  };

  return (
    <div className="pv-chart-wrap" ref={box}>
      <svg
        ref={props.svgRef}
        xmlns="http://www.w3.org/2000/svg"
        width={width}
        height={height}
        viewBox={`0 0 ${width} ${height}`}
        className="pv-chart"
        role="img"
        aria-label={props.title}
        fontFamily={ink.font}
        fontSize={FONT}
        onMouseMove={onMove}
        onMouseLeave={() => {
          setTip(null);
          setHoverX(null);
        }}
      >
        <rect x={0} y={0} width={width} height={height} fill={ink.surface} />
        <text x={16} y={22} fontSize={14} fontWeight={600} fill={ink.text}>
          {shorten(props.title, Math.floor((width - 32) / 7.5))}
        </text>
        {legend && <g transform="translate(16, 34)">{legend}</g>}
        {hasData ? (
          body.marks
        ) : (
          <text x={width / 2} y={top + 50} textAnchor="middle" fill={ink.muted}>
            —
          </text>
        )}
        {body.cursor?.(hoverX)}
      </svg>
      {tip && <Tooltip tip={tip} width={width} ink={ink} />}
    </div>
  );
}

// ---------- legend ----------

function legendRows(names: string[], width: number): number {
  let rows = 1;
  let x = 0;
  for (const n of names) {
    const w = 22 + Math.min(n.length, 24) * CHAR + 16;
    if (x + w > width - 32 && x > 0) {
      rows++;
      x = 0;
    }
    x += w;
  }
  return rows;
}

function Legend({ series, lineKeys, ink, width }: { series: { name: string; color: string }[]; lineKeys: boolean; ink: Ink; width: number }) {
  let x = 0;
  let y = 0;
  return (
    <g>
      {series.map((s, i) => {
        const label = shorten(s.name || "—", 24);
        const w = 22 + label.length * CHAR + 16;
        if (x + w > width - 32 && x > 0) {
          x = 0;
          y += 20;
        }
        const g = (
          <g key={i} transform={`translate(${x}, ${y})`}>
            {lineKeys ? (
              <line x1={0} x2={14} y1={7} y2={7} stroke={s.color} strokeWidth={2} strokeLinecap="round" />
            ) : (
              <rect x={0} y={1} width={12} height={12} rx={3} fill={s.color} />
            )}
            <text x={20} y={11} fill={ink.text}>
              <title>{s.name}</title>
              {label}
            </text>
          </g>
        );
        x += w;
        return g;
      })}
    </g>
  );
}

// ---------- tooltip ----------

function Tooltip({ tip, width, ink }: { tip: Tip; width: number; ink: Ink }) {
  const left = Math.min(tip.x + 14, width - 230);
  return (
    <div className="pv-chart-tip" style={{ left: Math.max(4, left), top: Math.max(4, tip.y - 8) }}>
      <div className="pv-chart-tip-title">{tip.title}</div>
      {tip.rows.map((r, i) => (
        <div key={i} className="pv-chart-tip-row">
          {r.key === "line" ? (
            <span className="pv-chart-tip-key line" style={{ background: r.color }} />
          ) : (
            <span className="pv-chart-tip-key" style={{ background: r.color, borderRadius: r.key === "dot" ? "50%" : 3 }} />
          )}
          <strong style={{ color: ink.text }}>{r.value}</strong>
          {r.name && <span className="pv-chart-tip-name">{r.name}</span>}
        </div>
      ))}
    </div>
  );
}

const tipRows = (props: ChartProps, colors: string[], i: number, key: TipRow["key"]): TipRow[] =>
  props.data.series.map((s, j) => ({ color: colors[j], key, name: s.name || props.yLabel, value: formatValue(s.values[i]) }));

// ---------- horizontal bars (categories) ----------

function horizontalBody(props: ChartProps, colors: string[], width: number, top: number) {
  const { data, ink, stacked } = props;
  const n = data.labels.length;
  const s = data.series.length;
  const grouped = s > 1 && !stacked;
  const thick = grouped ? (s <= 3 ? 12 : 8) : 20;
  const band = grouped ? s * thick + (s - 1) * 2 + 12 : 30;
  const maxChars = Math.max(...data.labels.map((l) => l.length), 4);
  const labelW = Math.min(Math.max(60, maxChars * CHAR + 8), Math.floor(width * 0.32));
  const valueRoom = props.showValues ? 56 : 20;
  const left = 16 + labelW + 10;
  const right = width - valueRoom;
  const [lo, hi] = valueExtent(data, stacked);
  const ticks = niceTicks(lo, hi);
  const t0 = ticks[0];
  const t1 = ticks[ticks.length - 1];
  const fmt = axisFormatter(ticks);
  const sx = (v: number) => left + ((v - t0) / (t1 - t0 || 1)) * (right - left);
  const plotTop = top + 6;
  const plotH = n * band;
  const base = sx(Math.max(t0, Math.min(0, t1)));
  const marks: React.ReactNode[] = [];

  ticks.forEach((tv) =>
    marks.push(
      <g key={`t${tv}`}>
        <line x1={sx(tv)} x2={sx(tv)} y1={plotTop} y2={plotTop + plotH} stroke={tv === 0 ? ink.axis : ink.grid} strokeWidth={1} />
        <text x={sx(tv)} y={plotTop + plotH + 16} textAnchor="middle" fill={ink.muted}>
          {fmt(tv)}
        </text>
      </g>,
    ),
  );
  data.labels.forEach((label, i) => {
    const y = plotTop + i * band;
    const special = i === data.other_label || label === "(empty)";
    marks.push(
      <text key={`l${i}`} x={left - 10} y={y + band / 2 + 4} textAnchor="end" fill={special ? ink.muted : ink.text} fontStyle={special ? "italic" : undefined}>
        <title>{label}</title>
        {shorten(label, Math.floor(labelW / CHAR))}
      </text>,
    );
    let pos = 0;
    let neg = 0;
    const visible = data.series.map((sr) => sr.values[i]).filter((v) => v !== null && Number.isFinite(v) && v !== 0) as number[];
    let seen = 0;
    data.series.forEach((sr, j) => {
      const v = sr.values[i];
      if (v === null || v === undefined || !Number.isFinite(v)) return;
      if (stacked && s > 1) {
        const from = v >= 0 ? pos : neg;
        const to = from + v;
        if (v >= 0) pos = to;
        else neg = to;
        seen += v !== 0 ? 1 : 0;
        const last = seen === visible.length;
        // A 2px surface gap between touching segments.
        const a = sx(from) + (from === 0 ? 0 : v >= 0 ? 1 : -1);
        const b = sx(to) - (last ? 0 : v >= 0 ? 1 : -1);
        marks.push(<path key={`b${i}-${j}`} d={barPath(true, a, b, y + (band - thick) / 2, thick, last)} fill={colors[j]} />);
      } else {
        const across = grouped ? y + 6 + j * (thick + 2) : y + (band - thick) / 2;
        marks.push(<path key={`b${i}-${j}`} d={barPath(true, base, sx(v), across, thick, true)} fill={colors[j]} />);
      }
    });
    if (props.showValues && (s === 1 || stacked)) {
      const total = s === 1 ? data.series[0].values[i] : pos + neg;
      if (total !== null && total !== undefined && Number.isFinite(total)) {
        const end = sx(total);
        marks.push(
          <text key={`v${i}`} x={total >= 0 ? end + 6 : end - 6} y={y + band / 2 + 4} textAnchor={total >= 0 ? "start" : "end"} fill={ink.muted}>
            {compactValue(total)}
          </text>,
        );
      }
    }
  });
  marks.push(
    <text key="axis" x={(left + right) / 2} y={plotTop + plotH + 36} textAnchor="middle" fill={ink.muted}>
      {props.yLabel}
    </text>,
  );

  const hit = (x: number, y: number): Tip | null => {
    if (y < plotTop || y > plotTop + plotH || x < 16 || x > width - 4) return null;
    const i = Math.floor((y - plotTop) / band);
    if (i < 0 || i >= n) return null;
    return { x, y, title: data.labels[i], rows: tipRows(props, colors, i, "rect") };
  };
  const cursor = () => null;
  return { height: 6 + plotH + 48, marks: <g>{marks}</g>, hit, cursor };
}

// ---------- vertical: columns, lines, areas, histogram ----------

function verticalBody(props: ChartProps, colors: string[], width: number, top: number) {
  const { data, ink, kind } = props;
  const n = data.labels.length;
  const s = data.series.length;
  const stacked = (props.stacked && s > 1 && kind !== "line") || false;
  const [lo, hi] = valueExtent(data, stacked);
  const ticks = niceTicks(lo, hi);
  const t0 = ticks[0];
  const t1 = ticks[ticks.length - 1];
  const fmt = axisFormatter(ticks);
  const tickChars = Math.max(...ticks.map((t) => fmt(t).length));
  const left = 16 + 16 + tickChars * CHAR + 8;
  const right = width - 18;
  const plotW = right - left;
  const band = plotW / Math.max(1, n);
  // X labels: horizontal when they fit, else rotated and thinned.
  const maxChars = Math.min(18, Math.max(...data.labels.map((l) => l.length), 1));
  // Horizontal labels, showing every k-th when they don't all fit; rotate
  // only when even every 6th wouldn't fit side by side.
  const need = maxChars * CHAR + 12;
  const thin = Math.max(1, Math.ceil(need / band));
  const horizontalFits = thin <= 6;
  const every = horizontalFits ? thin : Math.max(1, Math.ceil(16 / band));
  const xLabelBand = horizontalFits ? 22 : Math.min(maxChars, 18) * CHAR * 0.6 + 20;
  const plotH = 300;
  const plotTop = top + 8;
  const plotBottom = plotTop + plotH;
  const sy = (v: number) => plotBottom - ((v - t0) / (t1 - t0 || 1)) * plotH;
  const base = sy(Math.max(t0, Math.min(0, t1)));
  const cx = (i: number) => left + band * i + band / 2;
  const marks: React.ReactNode[] = [];

  ticks.forEach((tv) =>
    marks.push(
      <g key={`t${tv}`}>
        <line x1={left} x2={right} y1={sy(tv)} y2={sy(tv)} stroke={tv === 0 ? ink.axis : ink.grid} strokeWidth={1} />
        <text x={left - 8} y={sy(tv) + 4} textAnchor="end" fill={ink.muted}>
          {fmt(tv)}
        </text>
      </g>,
    ),
  );
  // Histogram edges share one number style (100 / 200, or 1.5K / 2K).
  const edgeFmt = axisFormatter(data.bin_edges);
  data.labels.forEach((label, i) => {
    if (i % every !== 0) return;
    const x = kind === "histogram" ? left + band * i : cx(i);
    const y = plotBottom + 16;
    const special = i === data.other_label || label === "(empty)";
    const shown = kind === "histogram" ? edgeFmt(data.bin_edges[i] ?? 0) : shorten(label, 18);
    marks.push(
      horizontalFits ? (
        <text key={`x${i}`} x={x} y={y} textAnchor="middle" fill={special ? ink.muted : ink.text} fontStyle={special ? "italic" : undefined}>
          <title>{label}</title>
          {shown}
        </text>
      ) : (
        <text key={`x${i}`} x={x} y={y} textAnchor="end" transform={`rotate(-35 ${x} ${y})`} fill={ink.text}>
          <title>{label}</title>
          {shown}
        </text>
      ),
    );
  });
  if (kind === "histogram" && n > 0) {
    const lastEdge = data.bin_edges[data.bin_edges.length - 1];
    if (lastEdge !== undefined && horizontalFits && n % every === 0) {
      marks.push(
        <text key="xlast" x={right} y={plotBottom + 16} textAnchor="middle" fill={ink.text}>
          {edgeFmt(lastEdge)}
        </text>,
      );
    }
  }

  if (kind === "bar" || kind === "histogram") {
    const grouped = s > 1 && !stacked;
    const thick = kind === "histogram" ? Math.max(1, band - 2) : grouped ? Math.min(24, Math.max(2, (band * 0.8 - (s - 1) * 2) / s)) : Math.min(24, Math.max(2, band * 0.7));
    for (let i = 0; i < n; i++) {
      let pos = 0;
      let neg = 0;
      const visible = data.series.filter((sr) => {
        const v = sr.values[i];
        return v !== null && Number.isFinite(v) && v !== 0;
      }).length;
      let seen = 0;
      data.series.forEach((sr, j) => {
        const v = sr.values[i];
        if (v === null || v === undefined || !Number.isFinite(v)) return;
        if (stacked) {
          const from = v >= 0 ? pos : neg;
          const to = from + v;
          if (v >= 0) pos = to;
          else neg = to;
          seen += v !== 0 ? 1 : 0;
          const last = seen === visible;
          const a = sy(from) - (from === 0 ? 0 : v >= 0 ? 1 : -1);
          const b = sy(to) + (last ? 0 : v >= 0 ? 1 : -1);
          marks.push(<path key={`b${i}-${j}`} d={barPath(false, a, b, cx(i) - thick / 2, thick, last)} fill={colors[j]} />);
        } else {
          const across = kind === "histogram" ? left + band * i + 1 : grouped ? cx(i) - (s * thick + (s - 1) * 2) / 2 + j * (thick + 2) : cx(i) - thick / 2;
          marks.push(<path key={`b${i}-${j}`} d={barPath(false, base, sy(v), across, thick, true)} fill={colors[j]} />);
        }
      });
      if (props.showValues && (s === 1 || stacked) && kind === "bar" && n <= 30) {
        const total = s === 1 ? data.series[0].values[i] : pos + neg;
        if (total !== null && total !== undefined && Number.isFinite(total)) {
          marks.push(
            <text key={`v${i}`} x={cx(i)} y={total >= 0 ? sy(total) - 6 : sy(total) + 14} textAnchor="middle" fill={ink.muted}>
              {compactValue(total)}
            </text>,
          );
        }
      }
    }
  } else {
    // line / area
    const cum = new Array(n).fill(0);
    data.series.forEach((sr, j) => {
      const pts = sr.values.map((v, i) => {
        if (v === null || v === undefined || !Number.isFinite(v)) return null;
        const y0 = stacked ? cum[i] : 0;
        const y1 = y0 + v;
        if (stacked) cum[i] = y1;
        return { x: cx(i), y: sy(y1), b: sy(y0) };
      });
      // Split at gaps (missing values break the line).
      const runs: { x: number; y: number; b: number }[][] = [];
      let run: { x: number; y: number; b: number }[] = [];
      for (const p of pts) {
        if (p) run.push(p);
        else if (run.length) {
          runs.push(run);
          run = [];
        }
      }
      if (run.length) runs.push(run);
      runs.forEach((r, k) => {
        const line = r.map((p, i) => `${i ? "L" : "M"}${p.x},${p.y}`).join(" ");
        if (kind === "area") {
          const area = `${line} ${[...r].reverse().map((p) => `L${p.x},${p.b}`).join(" ")} Z`;
          // Stacked bands are the data, so they are filled; a lone area is a wash.
          marks.push(<path key={`a${j}-${k}`} d={area} fill={colors[j]} fillOpacity={stacked ? 0.55 : 0.1} />);
        }
        marks.push(<path key={`l${j}-${k}`} d={line} fill="none" stroke={colors[j]} strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />);
      });
      // Dots mark each value on a sparse single line; with several series
      // they crowd, and hover shows the values anyway.
      const showDots = n <= 40 && s === 1;
      pts.forEach((p, i) => {
        if (!p) return;
        const lone = (pts[i - 1] ?? null) === null && (pts[i + 1] ?? null) === null;
        if (showDots || lone) marks.push(<circle key={`d${j}-${i}`} cx={p.x} cy={p.y} r={4} fill={colors[j]} stroke={ink.surface} strokeWidth={2} />);
      });
    });
  }
  marks.push(<line key="base" x1={left} x2={right} y1={base} y2={base} stroke={ink.axis} strokeWidth={1} />);
  marks.push(
    <text key="ylab" x={16} y={plotTop + plotH / 2} textAnchor="middle" transform={`rotate(-90 16 ${plotTop + plotH / 2})`} fill={ink.muted}>
      {props.yLabel}
    </text>,
  );
  marks.push(
    <text key="xlab" x={(left + right) / 2} y={plotBottom + xLabelBand + 18} textAnchor="middle" fill={ink.muted}>
      {props.xLabel}
    </text>,
  );

  const index = (x: number) => (x < left || x > right ? -1 : Math.min(n - 1, Math.max(0, Math.floor((x - left) / band))));
  const hit = (x: number, y: number): Tip | null => {
    if (y < plotTop - 4 || y > plotBottom + 4) return null;
    const i = index(x);
    if (i < 0) return null;
    return { x, y, title: data.labels[i], rows: tipRows(props, colors, i, kind === "line" ? "line" : "rect") };
  };
  // Lines and areas: a crosshair snapped to the nearest x; bars: the
  // hovered band is shaded.
  const cursor = (hx: number | null) => {
    if (hx === null) return null;
    const i = index(hx);
    if (i < 0) return null;
    if (kind === "line" || kind === "area") {
      return <line x1={cx(i)} x2={cx(i)} y1={plotTop} y2={plotBottom} stroke={ink.axis} strokeWidth={1} pointerEvents="none" />;
    }
    return <rect x={left + band * i} y={plotTop} width={band} height={plotH} fill={ink.text} fillOpacity={0.05} pointerEvents="none" />;
  };
  return { height: 8 + plotH + xLabelBand + 30, marks: <g>{marks}</g>, hit, cursor };
}

// ---------- scatter ----------

function scatterBody(props: ChartProps, colors: string[], width: number, top: number) {
  const { data, ink } = props;
  const xs: number[] = [];
  const ys: number[] = [];
  data.series.forEach((s) =>
    s.values.forEach((v, i) => {
      const x = s.xs[i];
      if (v !== null && x !== null && Number.isFinite(v) && Number.isFinite(x)) {
        xs.push(x);
        ys.push(v);
      }
    }),
  );
  const yt = niceTicks(Math.min(...ys), Math.max(...ys));
  const xt = niceTicks(Math.min(...xs), Math.max(...xs));
  const fy = axisFormatter(yt);
  const fx = axisFormatter(xt);
  const left = 16 + 16 + Math.max(...yt.map((t) => fy(t).length)) * CHAR + 8;
  const right = width - 18;
  const plotH = 320;
  const plotTop = top + 8;
  const plotBottom = plotTop + plotH;
  const sx = (v: number) => left + ((v - xt[0]) / (xt[xt.length - 1] - xt[0] || 1)) * (right - left);
  const sy = (v: number) => plotBottom - ((v - yt[0]) / (yt[yt.length - 1] - yt[0] || 1)) * plotH;
  const dense = xs.length > 1500;
  const marks: React.ReactNode[] = [];
  yt.forEach((tv) =>
    marks.push(
      <g key={`y${tv}`}>
        <line x1={left} x2={right} y1={sy(tv)} y2={sy(tv)} stroke={ink.grid} strokeWidth={1} />
        <text x={left - 8} y={sy(tv) + 4} textAnchor="end" fill={ink.muted}>
          {fy(tv)}
        </text>
      </g>,
    ),
  );
  xt.forEach((tv) =>
    marks.push(
      <g key={`x${tv}`}>
        <line x1={sx(tv)} x2={sx(tv)} y1={plotTop} y2={plotBottom} stroke={ink.grid} strokeWidth={1} />
        <text x={sx(tv)} y={plotBottom + 16} textAnchor="middle" fill={ink.muted}>
          {fx(tv)}
        </text>
      </g>,
    ),
  );
  const points: { x: number; y: number; j: number; vx: number; vy: number }[] = [];
  data.series.forEach((s, j) =>
    s.values.forEach((v, i) => {
      const x = s.xs[i];
      if (v === null || x === null || !Number.isFinite(v) || !Number.isFinite(x)) return;
      points.push({ x: sx(x), y: sy(v), j, vx: x, vy: v });
    }),
  );
  points.forEach((p, k) =>
    marks.push(
      <circle key={k} cx={p.x} cy={p.y} r={dense ? 3 : 4} fill={colors[p.j]} fillOpacity={dense ? 0.6 : 0.85} stroke={ink.surface} strokeWidth={dense ? 1 : 2} />,
    ),
  );
  marks.push(
    <text key="ylab" x={16} y={plotTop + plotH / 2} textAnchor="middle" transform={`rotate(-90 16 ${plotTop + plotH / 2})`} fill={ink.muted}>
      {props.yLabel}
    </text>,
  );
  marks.push(
    <text key="xlab" x={(left + right) / 2} y={plotBottom + 36} textAnchor="middle" fill={ink.muted}>
      {props.xLabel}
    </text>,
  );
  // The nearest point within 24px answers the hover.
  const hit = (x: number, y: number): Tip | null => {
    let best = -1;
    let bestD = 24 * 24;
    for (let k = 0; k < points.length; k++) {
      const d = (points[k].x - x) ** 2 + (points[k].y - y) ** 2;
      if (d < bestD) {
        bestD = d;
        best = k;
      }
    }
    if (best < 0) return null;
    const p = points[best];
    const s = data.series[p.j];
    return {
      x,
      y,
      title: s.name || props.title,
      rows: [
        { color: colors[p.j], key: "dot", name: props.yLabel, value: formatValue(p.vy) },
        { color: colors[p.j], key: "dot", name: props.xLabel, value: formatValue(p.vx) },
      ],
    };
  };
  const cursor = () => null;
  return { height: 8 + plotH + 48, marks: <g>{marks}</g>, hit, cursor };
}

// ---------- export ----------

/** The chart as a standalone SVG document. */
export function svgMarkup(svg: SVGSVGElement): string {
  const clone = svg.cloneNode(true) as SVGSVGElement;
  clone.setAttribute("xmlns", "http://www.w3.org/2000/svg");
  return `<?xml version="1.0" encoding="UTF-8"?>\n${clone.outerHTML}`;
}

/** The chart rendered to PNG at `scale`× (2 = sharp on high-DPI screens). */
export async function pngBytes(svg: SVGSVGElement, scale = 2): Promise<Uint8Array> {
  const w = Number(svg.getAttribute("width"));
  const h = Number(svg.getAttribute("height"));
  const blob = new Blob([svgMarkup(svg)], { type: "image/svg+xml" });
  const url = URL.createObjectURL(blob);
  try {
    const img = new Image();
    await new Promise<void>((resolve, reject) => {
      img.onload = () => resolve();
      img.onerror = () => reject(new Error("could not render the chart"));
      img.src = url;
    });
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(w * scale);
    canvas.height = Math.round(h * scale);
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("no canvas");
    ctx.scale(scale, scale);
    ctx.drawImage(img, 0, 0, w, h);
    const png = await new Promise<Blob | null>((r) => canvas.toBlob(r, "image/png"));
    if (!png) throw new Error("could not encode PNG");
    return new Uint8Array(await png.arrayBuffer());
  } finally {
    URL.revokeObjectURL(url);
  }
}
