// Port of search_dialog.py: non-modal full-table search with Next/Previous
// across all pages. Floats near the top-right without blocking the window.
import { useEffect, useMemo, useRef, useState } from "react";
import type { JSX } from "react";
import { api, errorMessage } from "../lib/api";
import { useUi } from "../lib/ui";
import { SearchNavigator, previewValue } from "./logic";
import type { SearchParams, SearchStep } from "./logic";
import "./dialogs.css";
import { Icon } from "../components/Icon";

export interface SearchPanelProps {
  tabId: number;
  columns: string[];
  initialQuery: string;
  /** Current cell, so Next/Prev start from it. */
  anchor: { row: number | null; col: number | null };
  /** Absolute row. */
  onJump(row: number, column: string): void;
  onClose(): void;
  /** Lets F3 / Shift+F3 in the main window drive the panel. */
  registerKeys?(api: { next(): void; prev(): void }): void;
}

export function SearchPanel(props: SearchPanelProps): JSX.Element {
  const ui = useUi();
  const t = ui.t;
  const [query, setQuery] = useState(props.initialQuery);
  const [column, setColumn] = useState<string | null>(null);
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [wholeCell, setWholeCell] = useState(false);
  const [regex, setRegex] = useState(false);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState(() =>
    props.initialQuery ? t("search.status.pending") : t("search.status.initial"),
  );
  const [statusIsError, setStatusIsError] = useState(false);
  const [matchInfo, setMatchInfo] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  const tabRef = useRef(props.tabId);
  tabRef.current = props.tabId;
  const nav = useMemo(() => new SearchNavigator((o) => api.find(tabRef.current, o)), []);

  // Everything next/prev read, kept in a ref so the registered key API stays valid.
  const live = useRef({ props, query, column, caseSensitive, wholeCell, regex, busy });
  live.current = { props, query, column, caseSensitive, wholeCell, regex, busy };

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  // A different table or column list invalidates the cached matches.
  const columnsKey = props.columns.join("\u0000");
  useEffect(() => {
    nav.reset();
    setMatchInfo("");
    setColumn((c) => (c !== null && !props.columns.includes(c) ? null : c));
    setStatus(t("search.status.initial"));
    setStatusIsError(false);
  }, [props.tabId, columnsKey]);

  useEffect(() => {
    const el = inputRef.current;
    if (el && props.initialQuery && props.initialQuery !== el.value) {
      setQuery(props.initialQuery);
      onParamChange(props.initialQuery);
      el.focus();
      el.select();
    }
  }, [props.initialQuery]);

  const onParamChange = (text: string) => {
    nav.reset();
    setMatchInfo("");
    setStatusIsError(false);
    setStatus(text.trim() ? t("search.status.pending") : t("search.status.empty"));
  };

  const show = (step: SearchStep) => {
    setStatusIsError(false);
    switch (step.kind) {
      case "hit":
        setStatus(t(step.more ? "search.status.match_more" : "search.status.match", { current: step.current, total: step.total }));
        setMatchInfo(
          t("search.match_info", {
            row: step.hit.row + 1,
            column: step.hit.column,
            value: `'${previewValue(step.hit.value)}'`,
          }),
        );
        live.current.props.onJump(step.hit.row, step.hit.column);
        break;
      case "last":
        setStatus(t("search.status.last", { current: step.current, total: step.total }));
        break;
      case "first":
        setStatus(t("search.status.first", { total: step.total }));
        break;
      case "none":
        setStatus(t("search.status.no_matches"));
        setMatchInfo("");
        break;
    }
  };

  const run = async (direction: "next" | "prev") => {
    const s = live.current;
    if (s.busy) return;
    if (!s.query) {
      setStatus(t("search.status.empty"));
      return;
    }
    const params: SearchParams = {
      query: s.query,
      column: s.column,
      case_sensitive: s.caseSensitive,
      whole_cell: s.wholeCell,
      regex: s.regex,
    };
    setBusy(true);
    live.current.busy = true;
    setStatus(t("search.status.searching"));
    try {
      const step = direction === "next" ? await nav.next(params, s.props.anchor) : await nav.prev(params, s.props.anchor);
      show(step);
    } catch (e) {
      // A failed query leaves the cache as it was; the next attempt retries.
      setStatus(t("search.status.failed", { error: errorMessage(e) }));
      setStatusIsError(true);
    } finally {
      setBusy(false);
      live.current.busy = false;
    }
  };

  useEffect(() => {
    props.registerKeys?.({ next: () => run("next"), prev: () => run("prev") });
  }, [props.registerKeys]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      props.onClose();
    } else if (e.key === "Enter" && (e.target as HTMLElement).tagName !== "BUTTON") {
      e.preventDefault();
      e.stopPropagation();
      run(e.shiftKey ? "prev" : "next");
    } else if (e.key === "F3") {
      e.preventDefault();
      e.stopPropagation();
      run(e.shiftKey ? "prev" : "next");
    }
  };

  return (
    <div
      className="dialog nonmodal pv-search-panel"
      role="dialog"
      aria-modal="false"
      aria-label={t("search.title")}
      onKeyDown={onKeyDown}
    >
      <div className="dialog-title">
        <span>{t("search.title")}</span>
        <button className="icon close" aria-label={t("btn.close")} title={t("findbar.tooltip.close")} onClick={props.onClose}>
          <Icon name="close" />
        </button>
      </div>
      <div className="dialog-body">
        <div className="form-grid">
          <label htmlFor="pv-search-input">{t("search.label.find")}</label>
          <input
            id="pv-search-input"
            ref={inputRef}
            type="text"
            placeholder={t("search.placeholder")}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              onParamChange(e.target.value);
            }}
          />
          <label htmlFor="pv-search-column">{t("search.label.column")}</label>
          <select
            id="pv-search-column"
            value={column ?? ""}
            onChange={(e) => {
              setColumn(e.target.value === "" ? null : e.target.value);
              onParamChange(query);
            }}
          >
            <option value="">{t("search.all_columns")}</option>
            {props.columns.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>
        </div>
        <div className="row">
          {(
            [
              [caseSensitive, setCaseSensitive, "search.option.case"],
              [wholeCell, setWholeCell, "search.option.whole"],
              [regex, setRegex, "search.option.regex"],
            ] as const
          ).map(([value, setter, key]) => (
            <label key={key} className="row">
              <input
                type="checkbox"
                checked={value}
                onChange={(e) => {
                  setter(e.target.checked);
                  onParamChange(query);
                }}
              />
              {t(key)}
            </label>
          ))}
        </div>
        <div className="row">
          <button disabled={busy} title={t("findbar.tooltip.prev")} onClick={() => run("prev")}>
            {t("search.btn.prev")}
          </button>
          <button className="primary" disabled={busy} title={t("findbar.tooltip.next")} onClick={() => run("next")}>
            {t("search.btn.next")}
          </button>
          <span className="spacer" />
          <button onClick={props.onClose}>{t("btn.close")}</button>
        </div>
        <div className={`pv-search-status small ${statusIsError ? "error-text" : "muted"}`} aria-live="polite">
          {status}
        </div>
        {matchInfo && <div className="pv-search-match small">{matchInfo}</div>}
      </div>
    </div>
  );
}
