import { useState } from "react";
import type { Ui } from "../lib/ui";
import type { Language } from "../lib/api";
import { errorMessage, api } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { SubmitOnEnter, useAutoFocus } from "./common";
import "./dialogs.css";

export function languageLabel(l: Language): string {
  return `${l.flag} ${l.native_name}`.trim();
}

/** Language combo box, shared with the settings dialog. */
export function LanguagePicker({
  languages,
  value,
  onChange,
  autoFocus,
}: {
  languages: Language[];
  value: string;
  onChange: (code: string) => void;
  autoFocus?: boolean;
}) {
  const ref = useAutoFocus<HTMLSelectElement>();
  return (
    <select ref={autoFocus ? ref : undefined} value={value} onChange={(e) => onChange(e.target.value)}>
      {languages.map((l) => (
        <option key={l.code} value={l.code}>
          {languageLabel(l)}
          {l.name && l.name !== l.native_name ? ` (${l.name})` : ""}
        </option>
      ))}
    </select>
  );
}

interface Callbacks {
  onLanguage(lang: string, translations: Record<string, string>): void;
}

function LanguageDialog({ ui, close, a }: { ui: Ui; close: () => void; a: Callbacks }) {
  const t = ui.t;
  const current = ui.settings.current_language || ui.boot.language;
  const [code, setCode] = useState(current);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    if (busy) return;
    if (code === current) return close();
    setBusy(true);
    try {
      const r = await api.setLanguage(code);
      a.onLanguage(r.language, r.translations);
      close();
    } catch (e) {
      setError(errorMessage(e));
      setBusy(false);
    }
  };

  return (
    <Dialog
      title={t("dialog.language.title")}
      onClose={close}
      onSubmit={submit}
      className="pv-dialog-narrow"
      footer={
        <>
          <button onClick={close}>{t("btn.cancel")}</button>
          <button className="primary" disabled={busy} onClick={submit}>
            {t("btn.apply")}
          </button>
        </>
      }
    >
      <SubmitOnEnter onSubmit={submit}>
        <label className="col">
          {t("language.label")}
          <LanguagePicker languages={ui.boot.languages} value={code} onChange={setCode} autoFocus />
        </label>
        {error && <div className="error-text">{error}</div>}
      </SubmitOnEnter>
    </Dialog>
  );
}

export function openLanguageSelector(ui: Ui, a: Callbacks): Promise<void> {
  return ui.showDialog<void>((close) => <LanguageDialog ui={ui} close={() => close()} a={a} />, undefined);
}
