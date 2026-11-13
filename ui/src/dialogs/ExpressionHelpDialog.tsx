import { useMemo, useState } from "react";
import type { Ui } from "../lib/ui";
import type { Translate } from "../lib/i18n";
import { Dialog } from "../components/Dialog";
import { Markdown, useAutoFocus } from "./common";
import { filterFunctions, formatSignature, groupFunctions } from "./logic";
import "./dialogs.css";

export function expressionGuideMarkdown(t: Translate): string {
  const parts = [
    `# ${t("expr.title")}`,
    `## ${t("expr.overview")}`,
    t("expr.overview_text"),
    `## ${t("expr.column_refs")}`,
    t("expr.column_refs_text"),
    '```\ndata[column_name]       # table + column\ndata["column name"]     # quoted for names with spaces\n```',
    `## ${t("expr.assignment")}`,
    t("expr.assignment_text"),
    "```\ndata[discount] = price * 0.15\ndata[full_name] = first_name || ' ' || last_name\n```",
    `## ${t("expr.literals")}`,
    t("expr.literals_text"),
    `## ${t("expr.operators")}`,
    t("expr.operators_text"),
    `## ${t("expr.methods")}`,
    t("expr.methods_text"),
    `## ${t("expr.examples")}`,
    t("expr.examples_text"),
    `## ${t("expr.autocomplete")}`,
    t("expr.autocomplete_text"),
    `## ${t("expr.errors")}`,
    t("expr.errors_text"),
  ];
  return parts.join("\n\n");
}

type Tab = "guide" | "functions";

function ExpressionHelpDialog({ ui, close }: { ui: Ui; close: () => void }) {
  const t = ui.t;
  const [tab, setTab] = useState<Tab>("guide");
  const [query, setQuery] = useState("");
  const guide = useMemo(() => expressionGuideMarkdown(t), [t]);
  const groups = useMemo(() => groupFunctions(filterFunctions(ui.boot.functions, query)), [ui.boot.functions, query]);
  const searchRef = useAutoFocus<HTMLInputElement>();
  const methods = Object.entries(ui.boot.methods ?? {}).filter(([, m]) => m.length > 0);

  return (
    <Dialog
      title={t("dialog.expression_help")}
      onClose={close}
      className="pv-dialog-wide pv-dialog-tall"
      footer={
        <button className="primary" onClick={close}>
          {t("btn.close")}
        </button>
      }
    >
      <div className="tabs-strip" role="tablist">
        <button role="tab" aria-selected={tab === "guide"} className={tab === "guide" ? "active" : ""} onClick={() => setTab("guide")}>
          {t("dialog.expr_help.guide")}
        </button>
        <button
          role="tab"
          aria-selected={tab === "functions"}
          className={tab === "functions" ? "active" : ""}
          onClick={() => setTab("functions")}
        >
          {t("dialog.expr_help.reference")}
        </button>
      </div>
      {tab === "guide" ? (
        <Markdown source={guide} />
      ) : (
        <div className="col pv-fill">
          <div className="pv-hint">{t("expr.functions_text")}</div>
          <input
            ref={searchRef}
            type="search"
            placeholder={t("dialog.expr_help.search")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          {groups.length === 0 && <div className="muted">{t("dialog.expr_help.no_results")}</div>}
          <div className="doc pv-selectable">
            {groups.map(([key, fs]) => (
              <section key={key}>
                <h3>{t(key)}</h3>
                <table className="simple pv-func-table">
                  <thead>
                    <tr>
                      <th>{t("dialog.expr_help.col.function")}</th>
                      <th>{t("dialog.expr_help.col.signature")}</th>
                      <th>{t("dialog.expr_help.col.description")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {fs.map((f) => (
                      <tr key={f.name}>
                        <td>
                          <code>{f.name}</code>
                        </td>
                        <td>
                          <code>{formatSignature(f)}</code>
                          {f.return_type && <div className="muted small">→ {f.return_type}</div>}
                        </td>
                        <td>
                          {f.description}
                          {f.params.length > 0 && (
                            <ul className="small">
                              {f.params.map((p) => (
                                <li key={p.name}>
                                  <code>{p.name}</code> <span className="muted">({p.logical_type})</span>
                                  {p.description ? ` — ${p.description}` : ""}
                                </li>
                              ))}
                            </ul>
                          )}
                          {f.example && (
                            <div className="small">
                              {t("dialog.expr_help.example")} <code>{f.example}</code>
                            </div>
                          )}
                          {f.sql_name && (
                            <div className="small muted">
                              SQL: <code>{f.sql_name}</code>
                            </div>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </section>
            ))}
            {methods.length > 0 && !query.trim() && (
              <section>
                <h3>{t("dialog.expr_help.methods_by_type")}</h3>
                <table className="simple">
                  <tbody>
                    {methods.map(([type, ms]) => (
                      <tr key={type}>
                        <td className="nowrap">
                          <code>{type}</code>
                        </td>
                        <td>
                          {ms.map((m, i) => (
                            <span key={m}>
                              {i > 0 && ", "}
                              <code>.{m}()</code>
                            </span>
                          ))}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </section>
            )}
          </div>
        </div>
      )}
    </Dialog>
  );
}

export function openExpressionHelp(ui: Ui): Promise<void> {
  return ui.showDialog<void>((close) => <ExpressionHelpDialog ui={ui} close={() => close()} />, undefined);
}
