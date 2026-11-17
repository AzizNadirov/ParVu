import { useEffect, useMemo, useRef, useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import { api, errorMessage, type Cell, type UniqueValues } from "../lib/api";
import { isCancelled } from "../lib/jobs";
import { Dialog } from "../components/Dialog";
import { JobProgress } from "../components/JobProgress";
import { SubmitOnEnter } from "./common";
import { filterUniqueValues, mergeUniqueValues, sortUniqueValues } from "./logic";
import "./dialogs.css";

/** Selection keys: NULL cannot be a Set key next to the string "null", so it gets its own. */
const NULL_KEY = "\u0000null";
const keyOf = (v: Cell) => (v === null ? NULL_KEY : v);

/** Rows drawn at most; searching narrows the rest. Thousands of checkboxes would stall the window. */
export const UNIQUE_SHOWN = 500;

type Scan =
  | { state: "running"; rows: number | null }
  | { state: "done"; result: UniqueValues }
  | { state: "stopped"; rows: number | null }
  | { state: "failed"; error: string };

interface Props {
  ui: Ui;
  close: Closer<Cell[] | null>;
  tabId: number;
  column: string;
  /** Rows of the table, for progress. */
  total: number;
}

function UniqueValuesDialog({ ui, close, tabId, column, total }: Props) {
  const t = ui.t;
  const [values, setValues] = useState<Cell[]>([]);
  const [scan, setScan] = useState<Scan>({ state: "running", rows: null });
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const cancel = useRef<() => void>(() => undefined);
  const search = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let live = true;
    // New values arrive a few times a second; merge them in batches.
    const job = api.uniqueValues(tabId, column, {
      onProgress: (p) => {
        if (!live) return;
        if (p.restart) setValues([]);
        if (Array.isArray(p.data) && p.data.length) setValues((v) => mergeUniqueValues(v, p.data as Cell[]));
        if (p.rows !== undefined) setScan((s) => (s.state === "running" ? { state: "running", rows: p.rows ?? null } : s));
      },
    });
    cancel.current = job.cancel;
    job.result.then(
      (result) => {
        if (!live) return;
        setValues(sortUniqueValues(result.values));
        setScan({ state: "done", result });
      },
      (e) => {
        if (!live) return;
        setScan((s) =>
          isCancelled(e) ? { state: "stopped", rows: s.state === "running" ? s.rows : null } : { state: "failed", error: errorMessage(e) },
        );
      },
    );
    return () => {
      live = false;
      job.cancel();
    };
  }, [tabId, column]);

  // The progress bar (and its Stop button) goes away when the scan ends:
  // keep the focus in the dialog so Escape and typing still work.
  const running = scan.state === "running";
  useEffect(() => {
    if (!running && !search.current?.closest("[role=dialog]")?.contains(document.activeElement)) search.current?.focus();
  }, [running]);

  const visible = useMemo(() => filterUniqueValues(values, query), [values, query]);
  const shown = visible.length > UNIQUE_SHOWN ? visible.slice(0, UNIQUE_SHOWN) : visible;

  const toggle = (v: Cell) =>
    setSelected((s) => {
      const n = new Set(s);
      const k = keyOf(v);
      if (n.has(k)) n.delete(k);
      else n.add(k);
      return n;
    });
  const selectVisible = () =>
    setSelected((s) => {
      const n = new Set(s);
      visible.forEach((v) => n.add(keyOf(v)));
      return n;
    });
  const finish = (result: Cell[] | null) => {
    cancel.current();
    close(result);
  };
  const submit = () => {
    if (selected.size === 0) return;
    finish(values.filter((v) => selected.has(keyOf(v))));
  };

  const summary = (() => {
    switch (scan.state) {
      case "running":
        return null;
      case "failed":
        return <div className="pv-callout">{t("error.unique_values_error_msg", { error: scan.error })}</div>;
      case "stopped":
        return <div className="pv-callout">{t("unique.stopped", { count: values.length })}</div>;
      case "done":
        if (scan.result.stopped === "too_many") return <div className="pv-callout">{t("unique.too_many", { limit: values.length })}</div>;
        if (scan.result.stopped === "too_large") return <div className="pv-callout">{t("unique.too_large", { count: values.length })}</div>;
        return <div>{t("unique.found", { count: values.length })}</div>;
    }
  })();

  return (
    <Dialog
      title={t("dialog.unique_values", { column_name: column })}
      onClose={() => finish(null)}
      onSubmit={submit}
      className="pv-dialog-narrow pv-dialog-tall pv-unique"
      footer={
        <>
          <span className="left muted small">{t("dialog.unique.selected", { count: selected.size })}</span>
          <button onClick={() => finish(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={selected.size === 0} onClick={submit}>
            {t("btn.apply_filter")}
          </button>
        </>
      }
    >
      <SubmitOnEnter onSubmit={submit}>
        {scan.state === "running" ? (
          <JobProgress
            t={t}
            label={t("job.unique_values", { column })}
            rows={scan.rows}
            total={total || null}
            detail={t("unique.scanning", { count: values.length })}
            onCancel={() => cancel.current()}
          />
        ) : (
          summary
        )}
        <input
          ref={search}
          type="search"
          placeholder={t("label.search_placeholder")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <div className="row">
          <button onClick={selectVisible} disabled={!visible.length}>
            {t("btn.select_all")}
          </button>
          <button onClick={() => setSelected(new Set())}>{t("btn.deselect_all")}</button>
        </div>
        <div className="list-box pv-list-fill" role="listbox" aria-multiselectable="true">
          {shown.map((v) => {
            const k = keyOf(v);
            return (
              <div key={k} className={`item ${selected.has(k) ? "selected" : ""}`}>
                <label>
                  <input type="checkbox" checked={selected.has(k)} onChange={() => toggle(v)} />
                  {v === null ? <span className="pv-null-value">NULL</span> : <span className="ellipsis">{v === "" ? " " : v}</span>}
                </label>
              </div>
            );
          })}
          {visible.length > shown.length && (
            <div className="item muted">{t("unique.refine", { shown: shown.length, count: visible.length })}</div>
          )}
          {visible.length === 0 && scan.state !== "running" && <div className="item muted">{t("dialog.unique.no_matches")}</div>}
        </div>
      </SubmitOnEnter>
    </Dialog>
  );
}

/** Pick values of a column to filter by; the values load while the dialog is open. */
export function openUniqueValues(ui: Ui, a: { tabId: number; column: string; total: number }): Promise<Cell[] | null> {
  return ui.showDialog<Cell[] | null>(
    (close) => <UniqueValuesDialog ui={ui} close={close} tabId={a.tabId} column={a.column} total={a.total} />,
    null,
  );
}
