// Pure helpers for drawing charts: axis ticks and number labels.

/** About `count` round tick values covering [min, max] (1, 2, 5 × 10ⁿ steps). */
export function niceTicks(min: number, max: number, count = 5): number[] {
  if (!Number.isFinite(min) || !Number.isFinite(max)) return [];
  if (min === max) {
    const pad = Math.abs(min) || 1;
    return niceTicks(min - pad, max + pad, count);
  }
  const span = max - min;
  const raw = span / Math.max(1, count);
  const mag = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? 10 * mag;
  const start = Math.floor(min / step) * step;
  const ticks: number[] = [];
  for (let v = start; v <= max + step * 1e-9; v += step) ticks.push(Number(v.toPrecision(12)));
  if (ticks[ticks.length - 1] < max) ticks.push(Number((ticks[ticks.length - 1] + step).toPrecision(12)));
  return ticks;
}

/** Short label for an axis number: 1 234, 1.5k, 2.3M, 0.25. */
export function formatTick(v: number): string {
  const a = Math.abs(v);
  if (a >= 1e9) return `${trim(v / 1e9)}B`;
  if (a >= 1e6) return `${trim(v / 1e6)}M`;
  if (a >= 1e4) return `${trim(v / 1e3)}k`;
  return trim(v);
}

function trim(v: number): string {
  return String(Number(v.toFixed(Math.abs(v) >= 100 ? 0 : Math.abs(v) >= 1 ? 2 : 4)));
}

/** Label shortened to `max` characters with an ellipsis. */
export function shorten(label: string, max: number): string {
  return label.length > max ? `${label.slice(0, Math.max(1, max - 1))}…` : label;
}
