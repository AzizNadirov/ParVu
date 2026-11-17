import { useEffect, useState } from "react";
import type { Ui, Closer } from "../lib/ui";
import { api, errorMessage } from "../lib/api";
import type { CustomDefinition, StepsDraft } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { Icon } from "../components/Icon";
import { stepText } from "../components/Panels";
import { RichText } from "./common";
import { stepsDescription } from "./logic";
import "./dialogs.css";

interface Args {
  tabId: number;
  table: string;
}

/** "Save steps as custom operation…": which steps can be part of it (and
 * why the others can't), which columns become parameters. Resolves with the
 * operation to review in the editor, or null. */
function SaveStepsDialog({ ui, close, a }: { ui: Ui; close: Closer<CustomDefinition | null>; a: Args }) {
  const t = ui.t;
  const [draft, setDraft] = useState<StepsDraft | null>(null);
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState("");

  useEffect(() => {
    api
      .customFromSteps(a.tabId, [])
      .then(setDraft)
      .catch((e) => setError(errorMessage(e)))
      .finally(() => setBusy(false));
  }, [a.tabId]);

  const toggle = (name: string) =>
    setChosen((s) => {
      const n = new Set(s);
      if (n.has(name)) n.delete(name);
      else n.add(name);
      return n;
    });

  const open = async () => {
    setBusy(true);
    setError("");
    try {
      const d = await api.customFromSteps(a.tabId, [...chosen]);
      if (!d.definition) return;
      const labels = d.steps.filter((s) => s.included).map((s) => stepText(t, s.label));
      close({
        ...d.definition,
        title: t("custom.steps.def_title", { table: a.table }),
        description: t("custom.steps.def_description", { steps: stepsDescription(labels) }),
      });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const usable = !!draft?.definition;
  return (
    <Dialog
      title={t("custom.steps.title")}
      onClose={() => close(null)}
      onSubmit={() => usable && void open()}
      className="pv-dialog-medium pv-save-steps"
      footer={
        <>
          <button onClick={() => close(null)}>{t("btn.cancel")}</button>
          <button className="primary" disabled={!usable || busy} onClick={() => void open()}>
            {t("custom.steps.open_editor")}
          </button>
        </>
      }
    >
      <div>
        <RichText text={t("custom.steps.intro", { table: a.table })} />
      </div>
      {busy && !draft && <div className="pv-hint">{t("custom.steps.reading")}</div>}
      {draft && (
        <>
          <span className="pv-section-title">{t("custom.steps.steps")}</span>
          <ul className="pv-custom-report pv-steps-notes">
            {draft.steps.map((s, i) => (
              <li key={i} className={s.included ? "pv-check-pass" : "pv-check-skipped"}>
                <Icon name={s.included ? "checkCircle" : "skipCircle"} size={16} />
                <span className="col">
                  <span>{stepText(t, s.label)}</span>
                  {s.note && <span className="pv-hint">{t(s.note.key, s.note.args)}</span>}
                </span>
              </li>
            ))}
          </ul>
          {!usable && <div className="error-text small">{t("custom.steps.none")}</div>}
          {usable && draft.columns.length > 0 && (
            <>
              <span className="pv-section-title">{t("custom.steps.columns")}</span>
              <div className="pv-hint">{t("custom.steps.columns_hint")}</div>
              <div className="list-box pv-steps-columns">
                {draft.columns.map((c) => (
                  <div key={c.name} className={`item ${chosen.has(c.name) ? "selected" : ""}`} title={c.fixed ? t(c.fixed.key, c.fixed.args) : undefined}>
                    <label className="row grow">
                      <input type="checkbox" disabled={!!c.fixed} checked={chosen.has(c.name) && !c.fixed} onChange={() => toggle(c.name)} />
                      <span className="pv-mono">{c.name}</span>
                      <span className="pv-hint">{c.dtype}</span>
                      <span className="grow" />
                      <span className="pv-hint">{c.fixed ? t(c.fixed.key, c.fixed.args) : chosen.has(c.name) ? t("custom.steps.as_parameter") : t("custom.steps.as_required")}</span>
                    </label>
                  </div>
                ))}
              </div>
            </>
          )}
        </>
      )}
      {error && <div className="error-text small">{error}</div>}
    </Dialog>
  );
}

export function openSaveSteps(ui: Ui, a: Args): Promise<CustomDefinition | null> {
  return ui.showDialog<CustomDefinition | null>((close) => <SaveStepsDialog ui={ui} close={close} a={a} />, null);
}
