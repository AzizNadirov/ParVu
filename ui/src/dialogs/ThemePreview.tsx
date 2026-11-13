// Live preview of a theme: its CSS variables applied to a small mock window.
import { useEffect, useState } from "react";
import type { CSSProperties } from "react";
import type { Translate } from "../lib/i18n";
import type { ThemeDetails } from "../lib/api";
import { api, errorMessage } from "../lib/api";
import { parseCssVariables } from "./logic";
import "./dialogs.css";

const SWATCHES: [string, string][] = [
  ["--pv-colors-background", "dialog.theme.color.background"],
  ["--pv-colors-foreground", "dialog.theme.color.foreground"],
  ["--pv-colors-accent-primary", "dialog.theme.color.accent"],
  ["--pv-colors-editor-keyword", "dialog.theme.color.keyword"],
  ["--pv-colors-table-header-background", "dialog.theme.color.header"],
  ["--pv-colors-table-selection", "dialog.theme.color.selection"],
];

const unquote = (s: string | undefined) => (s ?? "").replace(/^"(.*)"$/, "$1");

export function ThemePreview({ t, name, details }: { t: Translate; name: string | null; details?: ThemeDetails }) {
  const [vars, setVars] = useState<Record<string, string> | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setError(null);
    if (!name) {
      setVars(null);
      return;
    }
    api
      .themeCss(name)
      .then((css) => alive && setVars(parseCssVariables(css)))
      .catch((e) => {
        if (!alive) return;
        setVars(null);
        setError(errorMessage(e));
      });
    return () => {
      alive = false;
    };
  }, [name]);

  if (!name) return <div className="muted">{t("dialog.theme.none_selected")}</div>;
  if (error) return <div className="error-text">{error}</div>;
  if (!vars) return <div className="muted">{t("app.loading")}</div>;

  const font = (k: string) => `${unquote(vars[`--pv-layout-${k}-font-family`])} (${vars[`--pv-layout-${k}-font-size`] ?? "?"})`;

  return (
    <div className="col">
      <div className="pv-selectable">
        <div className="pv-section-title">
          {name} {details?.builtin && <span className="pv-tag">{t("dialog.theme.builtin")}</span>}
        </div>
        {details?.author && (
          <div>
            <b>{t("dialog.theme.author")}</b> {details.author}
          </div>
        )}
        {details?.description && (
          <div>
            <b>{t("dialog.theme.description")}</b> {details.description}
          </div>
        )}
      </div>
      <div className="pv-theme-preview" style={vars as CSSProperties} aria-hidden="true">
        <div className="pv-tp-menu">
          <span>File</span>
          <span>Edit</span>
          <span>Operations</span>
        </div>
        <div className="pv-tp-editor">
          <span className="pv-tp-kw">SELECT</span> name, <span className="pv-tp-num">42</span>{" "}
          <span className="pv-tp-kw">FROM</span> data <span className="pv-tp-kw">WHERE</span> city ={" "}
          <span className="pv-tp-str">'Baku'</span> <span className="pv-tp-com">-- comment</span>
        </div>
        <table>
          <thead>
            <tr>
              <th>id</th>
              <th>name</th>
              <th>city</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>1</td>
              <td>Aysel</td>
              <td>Baku</td>
            </tr>
            <tr>
              <td>2</td>
              <td>Ivan</td>
              <td>Moscow</td>
            </tr>
            <tr className="pv-tp-sel">
              <td>3</td>
              <td>Anna</td>
              <td>Berlin</td>
            </tr>
          </tbody>
        </table>
        <div className="pv-tp-buttons">
          <button tabIndex={-1}>{t("btn.execute")}</button>
          <button tabIndex={-1} className="primary">
            {t("btn.apply")}
          </button>
        </div>
        <div className="pv-tp-status">{t("status.ready")}</div>
      </div>
      <div className="pv-swatches pv-selectable">
        {SWATCHES.map(([v, key]) => (
          <div key={v} style={{ display: "contents" }}>
            <span className="pv-swatch" style={{ background: vars[v] }} />
            <span>{t(key)}</span>
            <code>{vars[v] ?? "—"}</code>
          </div>
        ))}
      </div>
      <div className="pv-selectable small">
        <div>
          {t("dialog.theme.font.ui")} {font("default")}
        </div>
        <div>
          {t("dialog.theme.font.code")} {font("code")}
        </div>
        <div>
          {t("dialog.theme.font.table")} {font("table")}
        </div>
      </div>
    </div>
  );
}
