// Port of export_dialog.py: target file, per-column include/cast, date
// formatting and format options; then runs the export with progress.
import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import type { Ui, Closer } from "../lib/ui";
import type { ColumnMeta, ExportFinished, ExportProgress, Quoting } from "../lib/api";
import { api, errorMessage } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { useAutoFocus } from "./common";
import {
  DATE_PRESETS,
  DELIMITERS,
  EXPORT_FILE_TYPES,
  TIMESTAMP_PRESETS,
  buildExportOptions,
  datesApplicable,
  exportValidation,
  fileTypeForPath,
  initialExportState,
  isTemporal,
  withExtension,
} from "./logic";
import type { ExportFileType, ExportFormState } from "./logic";
import "./dialogs.css";

interface Args {
  tabId: number;
  tableName: string;
  columns: ColumnMeta[];
  defaultDir?: string;
}

type Phase = { kind: "form" } | { kind: "running"; job: number | null; percent: number | null; cancelling: boolean };

const TYPE_LABEL_KEYS: Record<ExportFileType, string> = {
  csv: "dialog.export.type.csv",
  tsv: "dialog.export.type.tsv",
  parquet: "dialog.export.type.parquet",
  json: "dialog.export.type.json",
  jsonl: "dialog.export.type.jsonl",
  xlsx: "dialog.export.type.xlsx",
};

const TYPE_EXTENSIONS: Record<ExportFileType, string[]> = {
  csv: ["csv"],
  tsv: ["tsv"],
  parquet: ["parquet", "pq"],
  json: ["json"],
  jsonl: ["jsonl", "ndjson"],
  xlsx: ["xlsx"],
};

/**
 * Subscribe to export events before the job id is known, so a fast export's
 * "finished" event is not missed; events are buffered until `setJob`.
 */
async function subscribeExport(onProgress: (p: ExportProgress) => void, onFinished: (f: ExportFinished) => void) {
  let job: number | null = null;
  const pending: { p?: ExportProgress; f?: ExportFinished }[] = [];
  const unlisten: UnlistenFn[] = await Promise.all([
    listen<ExportProgress>("export-progress", (e) => {
      if (job === null) pending.push({ p: e.payload });
      else if (e.payload.job === job) onProgress(e.payload);
    }),
    listen<ExportFinished>("export-finished", (e) => {
      if (job === null) pending.push({ f: e.payload });
      else if (e.payload.job === job) onFinished(e.payload);
    }),
  ]);
  return {
    setJob(id: number) {
      job = id;
      for (const ev of pending.splice(0)) {
        if (ev.p && ev.p.job === id) onProgress(ev.p);
        if (ev.f && ev.f.job === id) onFinished(ev.f);
      }
    },
    stop() {
      unlisten.forEach((u) => u());
    },
  };
}

function ExportDialog({ ui, close, a }: { ui: Ui; close: Closer<ExportFinished | null>; a: Args }) {
  const t = ui.t;
  const compressions = ui.boot.parquet_compressions?.length ? ui.boot.parquet_compressions : ["snappy"];
  const castTypes = ui.boot.cast_types?.length ? ui.boot.cast_types : [""];
  const [s, setS] = useState<ExportFormState>(() => initialExportState(a.tableName, a.columns, a.defaultDir, compressions[0]));
  const [phase, setPhase] = useState<Phase>({ kind: "form" });
  const [error, setError] = useState<string | null>(null);
  const [pendingEdits, setPendingEdits] = useState(0);
  const sub = useRef<Awaited<ReturnType<typeof subscribeExport>> | null>(null);
  const pathRef = useAutoFocus<HTMLInputElement>(true);
  const jobRef = useRef<number | null>(null);

  useEffect(() => {
    api
      .tabSummary(a.tabId)
      .then((sum) => setPendingEdits(sum.pending_edits))
      .catch(() => undefined);
    return () => {
      // Closed while still running (e.g. the window went away): stop the job too.
      if (sub.current) {
        sub.current.stop();
        if (jobRef.current !== null) api.cancelExport(jobRef.current).catch(() => undefined);
      }
    };
  }, [a.tabId]);

  const set = <K extends keyof ExportFormState>(k: K, v: ExportFormState[K]) => setS((x) => ({ ...x, [k]: v }));
  const setType = (type: ExportFileType) => setS((x) => ({ ...x, type, path: withExtension(x.path, type) }));
  const setPath = (path: string) =>
    setS((x) => {
      const type = fileTypeForPath(path);
      return { ...x, path, type: type ?? x.type };
    });
  const setColumn = (i: number, patch: Partial<ExportFormState["columns"][number]>) =>
    setS((x) => ({ ...x, columns: x.columns.map((c, j) => (j === i ? { ...c, ...patch } : c)) }));
  const setAll = (include: boolean) => setS((x) => ({ ...x, columns: x.columns.map((c) => ({ ...c, include })) }));

  const browse = async () => {
    const path = await save({
      title: t("dialog.export_results"),
      defaultPath: s.path || undefined,
      filters: [
        { name: t(TYPE_LABEL_KEYS[s.type]), extensions: TYPE_EXTENSIONS[s.type] },
        ...EXPORT_FILE_TYPES.filter((x) => x !== s.type).map((x) => ({ name: t(TYPE_LABEL_KEYS[x]), extensions: TYPE_EXTENSIONS[x] })),
      ],
    });
    if (!path) return;
    const type = fileTypeForPath(path);
    setS((x) => ({ ...x, path: type ? path : withExtension(path, x.type), type: type ?? x.type }));
  };

  const invalid = exportValidation(s);
  const running = phase.kind === "running";
  const dates = datesApplicable(s.type, s.columns);
  const hasTemporal = s.columns.some((c) => isTemporal(c.dtype));
  const datesHint =
    s.type === "parquet"
      ? t("dialog.export.dates_parquet")
      : !hasTemporal
        ? t("dialog.export.dates_none")
        : t("dialog.export.dates_hint");

  const start = async () => {
    if (invalid || running) return;
    setError(null);
    const options = buildExportOptions(s);
    setPhase({ kind: "running", job: null, percent: null, cancelling: false });
    try {
      const subscription = await subscribeExport(
        (p) => setPhase((ph) => (ph.kind === "running" ? { ...ph, percent: p.percent } : ph)),
        (f) => {
          subscription.stop();
          sub.current = null;
          close(f);
        },
      );
      sub.current = subscription;
      const job = await api.startExport(a.tabId, options);
      jobRef.current = job;
      setPhase((ph) => (ph.kind === "running" ? { ...ph, job } : ph));
      subscription.setJob(job);
    } catch (e) {
      sub.current?.stop();
      sub.current = null;
      setPhase({ kind: "form" });
      setError(errorMessage(e));
    }
  };

  const cancel = async () => {
    if (phase.kind !== "running") return close(null);
    if (phase.job === null || phase.cancelling) return;
    setPhase({ ...phase, cancelling: true });
    try {
      await api.cancelExport(phase.job);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const percent = phase.kind === "running" ? phase.percent : null;
  const validationText =
    invalid === "no_path" ? t("dialog.export.need_path") : invalid === "no_columns" ? t("dialog.export.need_columns") : null;

  return (
    <Dialog
      title={t("dialog.export.title", { table: a.tableName })}
      onClose={cancel}
      onSubmit={start}
      className="pv-dialog-wide"
      footer={
        running ? (
          <>
            <div className="left grow">
              <div
                className={`pv-progress grow ${percent === null || percent < 0 ? "indeterminate" : ""}`}
                role="progressbar"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={percent !== null && percent >= 0 ? Math.round(percent) : undefined}
              >
                <div style={percent !== null && percent >= 0 ? { width: `${Math.min(100, percent)}%` } : undefined} />
              </div>
              <span className="nowrap small">
                {phase.cancelling
                  ? t("dialog.export.cancelling")
                  : percent !== null && percent >= 0
                    ? t("dialog.export.progress", { percent: Math.round(percent) })
                    : t("dialog.export.running")}
              </span>
            </div>
            <button onClick={cancel} disabled={phase.kind === "running" && (phase.job === null || phase.cancelling)}>
              {t("btn.cancel")}
            </button>
          </>
        ) : (
          <>
            <span className="left muted small">{validationText}</span>
            <button onClick={() => close(null)}>{t("btn.cancel")}</button>
            <button className="primary" disabled={!!invalid} onClick={start}>
              {t("dialog.export.export")}
            </button>
          </>
        )
      }
    >
      <fieldset disabled={running} className="col" style={{ border: "none", padding: 0 }}>
        <div className="form-grid">
          <label htmlFor="pv-export-path">{t("dialog.export.file")}</label>
          <div className="row">
            <input id="pv-export-path" ref={pathRef} type="text" className="grow" value={s.path} onChange={(e) => setPath(e.target.value)} />
            <button onClick={browse}>{t("dialog.export.browse")}</button>
          </div>
          <label htmlFor="pv-export-type">{t("dialog.export.format")}</label>
          <select id="pv-export-type" value={s.type} onChange={(e) => setType(e.target.value as ExportFileType)}>
            {EXPORT_FILE_TYPES.map((x) => (
              <option key={x} value={x}>
                {t(TYPE_LABEL_KEYS[x])}
              </option>
            ))}
          </select>
        </div>
        {pendingEdits > 0 && <div className="pv-hint">ℹ {t("dialog.export.pending_edits", { count: pendingEdits })}</div>}

        <fieldset className="col">
          <legend>{t("dialog.export.columns")}</legend>
          <div className="pv-table-scroll pv-export-columns">
            <table className="simple">
              <thead>
                <tr>
                  <th>{t("dialog.export.col.column")}</th>
                  <th>{t("dialog.export.col.source")}</th>
                  <th>{t("dialog.export.col.cast")}</th>
                </tr>
              </thead>
              <tbody>
                {s.columns.map((c, i) => (
                  <tr key={c.name}>
                    <td>
                      <label className="row">
                        <input type="checkbox" checked={c.include} onChange={(e) => setColumn(i, { include: e.target.checked })} />
                        {c.name}
                      </label>
                    </td>
                    <td className="pv-mono small">{c.dtype}</td>
                    <td>
                      <select
                        value={c.cast}
                        disabled={!c.include}
                        aria-label={t("dialog.export.col.cast")}
                        onChange={(e) => setColumn(i, { cast: e.target.value })}
                      >
                        {castTypes.map((ct) => (
                          <option key={ct} value={ct}>
                            {ct === "" ? t("dialog.export.keep_type") : ct}
                          </option>
                        ))}
                      </select>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <div className="row">
            <button onClick={() => setAll(true)}>{t("btn.select_all")}</button>
            <button onClick={() => setAll(false)}>{t("dialog.export.select_none")}</button>
            <span className="muted small">
              {t("dialog.export.selected_count", { count: s.columns.filter((c) => c.include).length, total: s.columns.length })}
            </span>
          </div>
        </fieldset>

        <fieldset className="col" disabled={!dates || running}>
          <legend>{t("dialog.export.dates")}</legend>
          <div className="form-grid">
            <label htmlFor="pv-export-date">{t("dialog.export.date_columns")}</label>
            <input
              id="pv-export-date"
              type="text"
              className="pv-mono"
              list="pv-export-date-presets"
              placeholder="%Y-%m-%d"
              value={s.dateFormat}
              onChange={(e) => set("dateFormat", e.target.value)}
            />
            <label htmlFor="pv-export-ts">{t("dialog.export.timestamp_columns")}</label>
            <input
              id="pv-export-ts"
              type="text"
              className="pv-mono"
              list="pv-export-ts-presets"
              placeholder="%Y-%m-%d %H:%M:%S"
              value={s.timestampFormat}
              onChange={(e) => set("timestampFormat", e.target.value)}
            />
          </div>
          <datalist id="pv-export-date-presets">
            {DATE_PRESETS.filter(Boolean).map((p) => (
              <option key={p} value={p} />
            ))}
          </datalist>
          <datalist id="pv-export-ts-presets">
            {TIMESTAMP_PRESETS.filter(Boolean).map((p) => (
              <option key={p} value={p} />
            ))}
          </datalist>
          <div className="pv-hint">{datesHint}</div>
        </fieldset>

        <fieldset className="col">
          <legend>{t("dialog.export.options")}</legend>
          {(s.type === "csv" || s.type === "tsv") && (
            <div className="form-grid">
              <label htmlFor="pv-export-delim">{t("dialog.export.delimiter")}</label>
              <select
                id="pv-export-delim"
                value={s.type === "tsv" ? "\t" : s.delimiter}
                disabled={s.type === "tsv"}
                onChange={(e) => set("delimiter", e.target.value)}
              >
                {DELIMITERS.map((d) => (
                  <option key={d.key} value={d.value}>
                    {`${t(d.key)}  ${d.symbol}`}
                  </option>
                ))}
              </select>
              <span />
              <label className="row">
                <input type="checkbox" checked={s.includeHeader} onChange={(e) => set("includeHeader", e.target.checked)} />
                {t("dialog.export.header")}
              </label>
              <label htmlFor="pv-export-quoting">{t("dialog.export.quoting")}</label>
              <select id="pv-export-quoting" value={s.quoting} onChange={(e) => set("quoting", e.target.value as Quoting)}>
                <option value="needed">{t("dialog.export.quoting.needed")}</option>
                <option value="all_valid">{t("dialog.export.quoting.all")}</option>
                <option value="none">{t("dialog.export.quoting.none")}</option>
              </select>
            </div>
          )}
          {s.type === "parquet" && (
            <div className="form-grid">
              <label htmlFor="pv-export-comp">{t("dialog.export.compression")}</label>
              <select id="pv-export-comp" value={s.compression} onChange={(e) => set("compression", e.target.value)}>
                {compressions.map((c) => (
                  <option key={c} value={c}>
                    {c}
                  </option>
                ))}
              </select>
            </div>
          )}
          {(s.type === "json" || s.type === "jsonl") && (
            <label className="row">
              <input type="checkbox" checked={s.type === "jsonl"} onChange={(e) => setType(e.target.checked ? "jsonl" : "json")} />
              {t("dialog.export.json_lines")}
            </label>
          )}
          {s.type === "xlsx" && <div className="pv-hint">{t("dialog.export.excel_hint")}</div>}
        </fieldset>
      </fieldset>
      {error && <div className="error-text">{t("error.export_error_msg", { error })}</div>}
    </Dialog>
  );
}

export function openExportDialog(ui: Ui, a: Args): Promise<ExportFinished | null> {
  return ui.showDialog<ExportFinished | null>((close) => <ExportDialog ui={ui} close={close} a={a} />, null);
}
