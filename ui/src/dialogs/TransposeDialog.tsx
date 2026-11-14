import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { ColumnMeta, Operation } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { ColumnSelect, RichText, SubmitOnEnter } from "./common";
import "./dialogs.css";

interface Args {
  table: string;
  columns: ColumnMeta[];
  rows: number;
}

function TransposeDialog({ ui, close, a }: { ui: Ui; close: Closer<Operation | null>; a: Args }) {
  const t = ui.t;
  const [header, setHeader] = useState("");
  const tooMany = a.rows > 500;
  const submit = () => !tooMany && close({ op: "transpose", header_column: header || null });

  return (
    <Dialog
      title={t("dialog.transpose.title")}
      onClose={() => close(null)}
      onSubmit={submit}
      className="pv-dialog-narrow"
      footer={
        <>
          <button onClick={() => close(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={tooMany} onClick={submit}>
            {t("btn.apply")}
          </button>
        </>
      }
    >
      <SubmitOnEnter onSubmit={submit}>
        <div>
          <RichText text={t("dialog.common.table", { table: a.table })} />
        </div>
        <label className="row">
          {t("dialog.transpose.header")}
          <ColumnSelect
            columns={a.columns.map((c) => c.name)}
            value={header}
            onChange={setHeader}
            emptyLabel={t("dialog.transpose.row_numbers")}
          />
        </label>
        <div className="pv-hint">{t(tooMany ? "dialog.transpose.too_many" : "dialog.transpose.hint", { rows: a.rows })}</div>
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openTranspose(ui: Ui, a: Args): Promise<Operation | null> {
  return ui.showDialog<Operation | null>((close) => <TransposeDialog ui={ui} close={close} a={a} />, null);
}
