import { jobResult } from "../lib/jobs";
import { useEffect, useRef, useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import { api, errorMessage } from "../lib/api";
import type { JsonKey, Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { ColumnChecklist, RichText } from "./common";
import "./dialogs.css";

interface Args {
  tabId: number;
  table: string;
  column: string;
}

/** Turn keys of a JSON object column into columns. The keys come from the
 * first rows; "Scan all rows" looks at every row. */
function ExpandJsonDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const [keys, setKeys] = useState<JsonKey[] | null>(null);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [keep, setKeep] = useState(false);
  const [scanning, setScanning] = useState(false);
  const [full, setFull] = useState(false);
  const [error, setError] = useState("");

  const running = useRef<(() => void) | null>(null);
  const load = async (all: boolean) => {
    setScanning(true);
    const job = api.jsonKeys(a.tabId, a.column, all, all ? { label: t("job.json_keys", { column: a.column }) } : undefined);
    running.current = job.cancel;
    try {
      const k = await jobResult(job);
      if (k === null) return;
      setKeys(k);
      // Keep the user's choice for known keys; new keys start ticked.
      setChecked((prev) => new Set(k.map((x) => x.name).filter((n) => !keys || prev.has(n) || !keys.some((o) => o.name === n))));
      setFull(all);
      setError("");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      running.current = null;
      setScanning(false);
    }
  };
  useEffect(() => {
    void load(false);
    // A scan still running when the dialog closes is stopped.
    return () => running.current?.();
    // Once per dialog.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const chosen = (keys ?? []).filter((k) => checked.has(k.name));
  const op: Operation | null = chosen.length ? { op: "expand_json", column: a.column, keys: chosen, keep_original: keep } : null;
  const submit = () => op && close(op);
  const label = (k: JsonKey) => `${k.name}  (${t(`json.type.${k.dtype.toLowerCase()}`)})`;

  return (
    <Dialog
      title={t("json.expand.title")}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-narrow"
      footer={
        <>
          <div className="left">
            <button className="text" disabled={scanning || full} onClick={() => void load(true)}>
              {t("json.expand.scan_all")}
            </button>
          </div>
          <button onClick={() => close(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={!op} onClick={submit}>
            {t("btn.apply")}
          </button>
        </>
      }
    >
      <div>
        <RichText text={t("dialog.convert.column", { column: a.column, table: a.table })} />
      </div>
      {keys === null ? (
        <div className="muted">{t("json.expand.loading")}</div>
      ) : keys.length === 0 ? (
        <div className="muted">{t("json.expand.none")}</div>
      ) : (
        <>
          <span>{t(full ? "json.expand.keys_all" : "json.expand.keys_sample")}</span>
          <ColumnChecklist
            ui={ui}
            columns={keys.map((k) => k.name)}
            checked={checked}
            onChange={setChecked}
          />
          <div className="muted small">{keys.map(label).join(" · ")}</div>
        </>
      )}
      <label className="row">
        <input type="checkbox" checked={keep} onChange={(e) => setKeep(e.target.checked)} />
        {t("json.expand.keep")}
      </label>
      {error && <div className="error-text small">{error}</div>}
    </Dialog>
  );
}

export function openExpandJson(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <ExpandJsonDialog ui={ui} close={close} a={a} />, null);
}
