// Material ripple: a circle that grows from the pointer when a button, menu
// item or tab is pressed. One listener for the whole window; the ripple is a
// span removed when its animation ends. Skipped when reduced motion is asked.

const TARGETS = "button:not(:disabled), .menu-item:not(.disabled), .tab";

export function installRipple(root: Window = window): () => void {
  const reduced = root.matchMedia?.("(prefers-reduced-motion: reduce)");
  const onDown = (e: PointerEvent) => {
    if (e.button !== 0 || reduced?.matches) return;
    const host = (e.target as HTMLElement | null)?.closest?.<HTMLElement>(TARGETS);
    if (!host || host.classList.contains("no-ripple")) return;
    const r = host.getBoundingClientRect();
    const size = Math.hypot(r.width, r.height) * 2;
    const dot = document.createElement("span");
    dot.className = "md-ripple";
    dot.style.width = dot.style.height = `${size}px`;
    dot.style.left = `${e.clientX - r.left - size / 2}px`;
    dot.style.top = `${e.clientY - r.top - size / 2}px`;
    host.appendChild(dot);
    dot.addEventListener("animationend", () => dot.remove(), { once: true });
  };
  root.addEventListener("pointerdown", onDown, true);
  return () => root.removeEventListener("pointerdown", onDown, true);
}
