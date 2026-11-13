import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { RichText, SubmitOnEnter } from "./common";
import { buildAppendOp } from "./logic";
import "./dialogs.css";

interface Args {
  table: string;
  others: string[];
}

function AppendDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const [checked, setChecked] = useState<Set<string>>(() => new Set());
  const [distinct, setDistinct] = useState(false);
  const op = buildAppendOp(
    a.others.filter((o) => checked.has(o)),
    distinct,
  );
  const submit = () => op && close(op);
  const toggle = (name: string) =>
    setChecked((s) => {
      const n = new Set(s);
      if (n.has(name)) n.delete(name);
      else n.add(name);
      return n;
    });

  return (
    <Dialog
      title={t("dialog.append.title")}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-narrow"
      footer={
        <>
          <button onClick={() => close(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={!op} onClick={submit}>
            {t("btn.ok")}
          </button>
        </>
      }
    >
      <SubmitOnEnter onSubmit={submit}>
        <div>
          <RichText text={t("dialog.append.base_table", { table: a.table })} />
        </div>
        <div>{t("dialog.append.select")}</div>
        <div className="list-box">
          {a.others.map((name) => (
            <div key={name} className={`item ${checked.has(name) ? "selected" : ""}`}>
              <label className="row grow">
                <input type="checkbox" checked={checked.has(name)} onChange={() => toggle(name)} />
                {name}
              </label>
            </div>
          ))}
          {a.others.length === 0 && <div className="item muted">{t("dialog.append.no_tables")}</div>}
        </div>
        <fieldset className="col">
          <legend>{t("dialog.append.mode")}</legend>
          <label className="row">
            <input type="radio" name="pv-append-mode" checked={!distinct} onChange={() => setDistinct(false)} />
            {t("dialog.append.union_all")}
          </label>
          <label className="row">
            <input type="radio" name="pv-append-mode" checked={distinct} onChange={() => setDistinct(true)} />
            {t("dialog.append.union")}
          </label>
        </fieldset>
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openAppendDialog(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <AppendDialog ui={ui} close={close} a={a} />, null);
}
