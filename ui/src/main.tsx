import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { api, errorMessage, type Bootstrap } from "./lib/api";
import { applyThemeCss } from "./lib/theme";
import { App } from "./App";
import "./styles.css";
import "./app.css";
import "./dialogs/dialogs.css";

// Fetched once per page: the backend hands out one-time data (command-line
// files, crash report) on the first call only.
const bootstrapOnce = api.bootstrap();

function Root() {
  const [boot, setBoot] = useState<Bootstrap | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    bootstrapOnce
      .then((b) => {
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
