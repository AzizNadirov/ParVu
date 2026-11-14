import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { api, errorMessage, type Bootstrap } from "./lib/api";
import { applyThemeCss } from "./lib/theme";
import { installRipple } from "./lib/ripple";
import { App } from "./App";
import "@fontsource/roboto/400.css";
import "@fontsource/roboto/500.css";
import "@fontsource/roboto/700.css";
import "@fontsource/roboto-mono/400.css";
import "./styles.css";
import "./app.css";
import "./dialogs/dialogs.css";

// The grid draws text on a canvas, which does not redraw when a web font
// arrives, so the bundled fonts are loaded before the first render (at most
// a short wait; a missing font just falls back).
function loadFonts(): Promise<unknown> {
  const fonts = document.fonts;
  if (!fonts?.load) return Promise.resolve();
  const all = Promise.all(
    ["400 13px Roboto", "500 13px Roboto", "700 13px Roboto", "400 13px 'Roboto Mono'"].map((f) => fonts.load(f).catch(() => [])),
  );
  return Promise.race([all, new Promise((r) => setTimeout(r, 1500))]);
}

// Fetched once per page: the backend hands out one-time data (command-line
// files, crash report) on the first call only.
const bootstrapOnce = api.bootstrap();

function Root() {
  const [boot, setBoot] = useState<Bootstrap | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    Promise.all([bootstrapOnce, loadFonts()])
      .then(([b]) => {
        applyThemeCss(b.theme.css);
        document.documentElement.lang = b.language;
        setBoot(b);
      })
      .catch((e) => setError(errorMessage(e)));
  }, []);
  if (error) return <pre className="error-text" style={{ padding: 20 }}>{error}</pre>;
  if (!boot) return null;
  return <App boot={boot} />;
}

installRipple();

// Block the webview's own context menu everywhere except text inputs.
window.addEventListener("contextmenu", (e) => {
  const el = e.target as HTMLElement;
  if (!el.closest("input, textarea, .cm-editor")) e.preventDefault();
});

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Root />
  </StrictMode>,
);
