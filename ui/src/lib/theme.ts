// Applies a theme's CSS variables (generated in Rust) to the document.
const STYLE_ID = "parvu-theme";

export function applyThemeCss(css: string): void {
  let el = document.getElementById(STYLE_ID) as HTMLStyleElement | null;
  if (!el) {
    el = document.createElement("style");
    el.id = STYLE_ID;
    document.head.appendChild(el);
  }
  el.textContent = css;
}

/** Read a theme variable (e.g. "--pv-colors-table-background"). */
export function cssVar(name: string, fallback = ""): string {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || fallback;
}

/** A CSS colour (e.g. `var(--md-on-surface-variant)`, which may be a
 * `color-mix`) resolved to `rgb(...)`, for canvas drawing. */
export function resolvedColor(css: string, fallback: string): string {
  const probe = document.createElement("span");
  probe.style.color = css;
  probe.style.display = "none";
  document.body.appendChild(probe);
  const c = getComputedStyle(probe).color;
  probe.remove();
  return c || fallback;
}
