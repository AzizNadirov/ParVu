import { useCallback, useEffect, useState } from "react";
import type { Ui } from "../lib/ui";
import type { ThemeDetails, ThemeInfo } from "../lib/api";
import { api, errorMessage } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { ThemePreview } from "./ThemePreview";
import { useAutoFocus } from "./common";
import { deleteThemeFlow, exportThemeFlow, importThemeFlow } from "./themeActions";
import "./dialogs.css";

interface Callbacks {
  onTheme(info: ThemeInfo): void;
}

function ThemeSelectorDialog({ ui, close, a }: { ui: Ui; close: () => void; a: Callbacks }) {
  const t = ui.t;
  const current = ui.settings.current_theme || ui.boot.theme.name;
  const [themes, setThemes] = useState<ThemeDetails[]>([]);
  const [selected, setSelected] = useState<string | null>(current);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const listRef = useAutoFocus<HTMLDivElement>();

  const reload = useCallback(async (select?: string | null) => {
    try {
      const list = await api.listThemes();
      setThemes(list);
      setSelected((s) => {
        const want = select !== undefined ? select : s;
        return want && list.some((x) => x.name === want) ? want : list[0]?.name ?? null;
      });
    } catch (e) {
      setError(errorMessage(e));
    }
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  const details = themes.find((x) => x.name === selected);

  const apply = async () => {
    if (!selected || busy) return;
    setBusy(true);
    try {
      const info = await api.setTheme(selected);
      a.onTheme(info);
      close();
    } catch (e) {
      setError(errorMessage(e));
      setBusy(false);
    }
  };

  const onListKey = (e: React.KeyboardEvent) => {
    const i = themes.findIndex((x) => x.name === selected);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = themes[Math.max(0, Math.min(themes.length - 1, i + (e.key === "ArrowDown" ? 1 : -1)))];
      if (next) setSelected(next.name);
    } else if (e.key === "Enter") {
      e.preventDefault();
      apply();
    }
  };

  return (
    <Dialog
      title={t("dialog.theme_selector")}
      onClose={close}
      className="pv-dialog-wide"
      footer={
        <>
          <div className="left">
            <button
              onClick={async () => {
                const name = await importThemeFlow(ui);
                if (name) reload(name);
              }}
            >
              {t("btn.import")}
            </button>
            <button onClick={() => exportThemeFlow(ui, selected)} disabled={!selected}>
              {t("btn.export")}
            </button>
            <button
              className="danger"
              disabled={!details || details.builtin || details.name === current}
              title={details?.builtin ? t("theme.selector.delete_failed_msg") : undefined}
              onClick={async () => {
                if (selected && (await deleteThemeFlow(ui, selected))) reload(current);
              }}
            >
              {t("btn.delete")}
            </button>
          </div>
          <button onClick={close}>{t("btn.cancel")}</button>
          <button className="primary" disabled={!selected || busy} onClick={apply}>
            {t("btn.apply")}
          </button>
        </>
      }
    >
      <div className="pv-section-title">{t("theme.selector.title")}</div>
      {error && <div className="error-text">{error}</div>}
      <div className="pv-theme-layout">
        <div className="col">
          <span>{t("theme.selector.available")}</span>
          <div
            className="list-box pv-theme-list"
            role="listbox"
            tabIndex={0}
            ref={listRef}
            aria-activedescendant={selected ? `pv-theme-${selected}` : undefined}
            onKeyDown={onListKey}
          >
            {themes.map((th) => (
              <div
                key={th.name}
                id={`pv-theme-${th.name}`}
                role="option"
                aria-selected={th.name === selected}
                className={`item ${th.name === selected ? "selected" : ""}`}
                onClick={() => setSelected(th.name)}
                onDoubleClick={apply}
              >
                <span className="grow ellipsis">{th.name}</span>
                {th.name === current && <span className="pv-tag">{t("dialog.theme.current")}</span>}
              </div>
            ))}
          </div>
        </div>
        <fieldset>
          <legend>{t("theme.selector.preview")}</legend>
          <ThemePreview t={t} name={selected} details={details} />
        </fieldset>
      </div>
    </Dialog>
  );
}

export function openThemeSelector(ui: Ui, a: Callbacks): Promise<void> {
  return ui.showDialog<void>((close) => <ThemeSelectorDialog ui={ui} close={() => close()} a={a} />, undefined);
}
