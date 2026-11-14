import { useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import type { ColumnMeta } from "../lib/api";
import { FORMAT_COLORS, HIGHLIGHT_OPS } from "../lib/format";
import type { FormatColor, FormatRule, HighlightOp } from "../lib/format";
import { Dialog } from "../components/Dialog";
import { ColumnSelect } from "./common";
import { isNumericType } from "./logic";
import "./dialogs.css";
import { Icon } from "../components/Icon";

interface Args {
  columns: ColumnMeta[];
  rules: FormatRule[];
  selected?: string;
}

const SWATCH: Record<FormatColor, string> = {
  primary: "var(--pv-colors-accent-primary)",
  secondary: "var(--pv-colors-accent-secondary)",
  warning: "var(--pv-colors-accent-warning)",
  error: "var(--pv-colors-accent-error)",
};

/** Add and remove conditional formatting rules; resolves with the new list. */
function ConditionalFormatDialog({ ui, close, a }: { ui: Ui; close: Closer<FormatRule[] | null>; a: Args }) {
  const t = ui.t;
  const names = a.columns.map((c) => c.name);
  const [rules, setRules] = useState<FormatRule[]>(a.rules);
  const [column, setColumn] = useState(a.selected ?? names[0] ?? "");
  const [kind, setKind] = useState<FormatRule["kind"]>(() =>
    isNumericType(a.columns.find((c) => c.name === (a.selected ?? names[0]))?.dtype ?? "") ? "scale" : "highlight",
  );
  const [op, setOp] = useState<HighlightOp>("gt");
  const [value, setValue] = useState("");
  const [value2, setValue2] = useState("");
  const [color, setColor] = useState<FormatColor>("primary");
  const needsValue = kind === "highlight" && op !== "is_null";
  const canAdd = !!column && (!needsValue || (value.trim() !== "" && (op !== "between" || value2.trim() !== "")));
  const add = () => {
    if (!canAdd) return;
    const rule: FormatRule =
      kind === "scale" ? { kind, column, color } : { kind, column, op, value: value.trim(), value2: value2.trim(), color };
    setRules((l) => [...l, rule]);
    setValue("");
    setValue2("");
  };
  const describe = (r: FormatRule) =>
    r.kind === "scale"
      ? t("dialog.format.describe_scale", { column: r.column })
      : t("dialog.format.describe_rule", {
          column: r.column,
          condition: `${t(`format.op.${r.op}`)} ${r.op === "is_null" ? "" : r.value}${r.op === "between" ? ` – ${r.value2}` : ""}`.trim(),
        });

  return (
    <Dialog
      title={t("dialog.format.title")}
      onClose={() => close(null)}
      className="pv-dialog-medium"
      footer={
        <>
          <button onClick={() => close(null)}>{t("btn.cancel")}</button>
          <button className="primary" onClick={() => close(rules)}>
            {t("btn.ok")}
          </button>
        </>
      }
    >
      <div className="pv-section-title">{t("dialog.format.rules")}</div>
      <div className="list-box pv-column-checklist">
        {rules.length === 0 && <div className="item pv-hint">{t("dialog.format.none")}</div>}
        {rules.map((r, i) => (
          <div key={i} className="item row">
            <span className="pv-swatch" style={{ background: SWATCH[r.color] }} />
            <span className="grow">{describe(r)}</span>
            <button className="pv-icon-btn" title={t("btn.remove")} onClick={() => setRules((l) => l.filter((_, j) => j !== i))}>
              <Icon name="close" />
            </button>
          </div>
        ))}
      </div>
      <fieldset className="col">
        <legend>{t("dialog.format.add")}</legend>
        <label className="row">
          {t("dialog.common.column")}
          <ColumnSelect columns={names} value={column} onChange={setColumn} />
        </label>
        <label className="row">
          <input type="radio" name="format-kind" checked={kind === "scale"} onChange={() => setKind("scale")} />
          {t("dialog.format.scale")}
        </label>
        <label className="row">
          <input type="radio" name="format-kind" checked={kind === "highlight"} onChange={() => setKind("highlight")} />
          {t("dialog.format.highlight")}
        </label>
        {kind === "highlight" && (
          <div className="row pv-indent">
            <select value={op} onChange={(e) => setOp(e.target.value as HighlightOp)}>
              {HIGHLIGHT_OPS.map((o) => (
                <option key={o} value={o}>
                  {t(`format.op.${o}`)}
                </option>
              ))}
            </select>
            {needsValue && <input type="text" className="grow" value={value} onChange={(e) => setValue(e.target.value)} />}
            {op === "between" && (
              <>
                {t("dialog.filter.and")}
                <input type="text" className="grow" value={value2} onChange={(e) => setValue2(e.target.value)} />
              </>
            )}
          </div>
        )}
        <div className="row">
          {t("dialog.format.color")}
          {FORMAT_COLORS.map((c) => (
            <label key={c} className="row" title={t(`format.color.${c}`)}>
              <input type="radio" name="format-color" checked={color === c} onChange={() => setColor(c)} />
              <span className="pv-swatch" style={{ background: SWATCH[c] }} />
            </label>
          ))}
          <span className="grow" />
          <button disabled={!canAdd} onClick={add}>
            {t("dialog.format.add_rule")}
          </button>
        </div>
      </fieldset>
      <div className="pv-hint">{t("dialog.format.hint")}</div>
    </Dialog>
  );
}

export function openConditionalFormat(ui: Ui, a: Args): Promise<FormatRule[] | null> {
  return ui.showDialog<FormatRule[] | null>((close) => <ConditionalFormatDialog ui={ui} close={close} a={a} />, null);
}
