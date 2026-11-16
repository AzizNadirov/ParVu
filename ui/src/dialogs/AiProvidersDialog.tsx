import { useEffect, useState } from "react";
import type { Ui } from "../lib/ui";
import { api, errorMessage } from "../lib/api";
import type { Effort, ProviderConfig, ProvidersInfo } from "../lib/api";
import { PROVIDER_PRESETS, providerFromPreset, providerProblem } from "../lib/ai";
import { Dialog } from "../components/Dialog";
import "./dialogs.css";

const EFFORTS: Effort[] = ["low", "medium", "high", "xhigh", "max"];

/** AI → AI providers: add, edit, test and remove the models the assistant uses. */
function AiProvidersDialog({ ui, close }: { ui: Ui; close: () => void }) {
  const t = ui.t;
  const [info, setInfo] = useState<ProvidersInfo | null>(null);
  const [edit, setEdit] = useState<ProviderConfig | null>(null);
  const [isNew, setIsNew] = useState(false);
  const [key, setKey] = useState("");
  const [preset, setPreset] = useState(PROVIDER_PRESETS[0].key);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    void api.aiProviders().then(setInfo);
  }, []);

  const view = edit && info?.providers.find((p) => p.config.id === edit.id);
  const presetOf = (c: ProviderConfig) =>
    PROVIDER_PRESETS.find((p) => p.kind === c.kind && (p.base_url ?? null) === (c.base_url ?? null)) ?? null;
  const models = edit ? presetOf(edit)?.models ?? [] : [];
  const problem = edit ? providerProblem(edit) : null;
  const set = (patch: Partial<ProviderConfig>) => setEdit((c) => (c ? { ...c, ...patch } : c));

  const add = () => {
    const p = PROVIDER_PRESETS.find((x) => x.key === preset) ?? PROVIDER_PRESETS[0];
    setEdit(providerFromPreset(p, info?.providers.map((v) => v.config.id) ?? []));
    setIsNew(true);
    setKey("");
    setMessage(null);
  };

  const save = async (andTest: boolean) => {
    if (!edit || problem) return;
    setBusy(true);
    setMessage(null);
    try {
      const next = await api.aiSaveProvider(edit, key.trim() ? key.trim() : undefined, false);
      setInfo(next);
      setIsNew(false);
      setKey("");
      if (andTest) {
        const model = await api.aiTestProvider(edit.id);
        setMessage({ ok: true, text: t("ai.providers.test_ok", { model }) });
      } else {
        setMessage({ ok: true, text: t("ai.providers.saved") });
      }
    } catch (e) {
      setMessage({ ok: false, text: errorMessage(e) });
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!edit) return;
    if (!(await ui.confirm(t("ai.providers.delete_title"), t("ai.providers.delete_confirm", { name: edit.display_name })))) return;
    setInfo(await api.aiDeleteProvider(edit.id));
    setEdit(null);
  };

  const makeDefault = async () => {
    if (!edit) return;
    setInfo(await api.aiSaveProvider(edit, undefined, true));
  };

  const forgetKey = async () => {
    if (!edit) return;
    setInfo(await api.aiSaveProvider(edit, "", false));
  };

  const keySource = view?.key_source ?? "none";

  return (
    <Dialog
      title={t("ai.providers.title")}
      onClose={close}
      className="pv-dialog-wide"
      footer={
        <button className="primary" onClick={close}>
          {t("btn.close")}
        </button>
      }
    >
      <div className="pv-hint">{t("ai.providers.intro")}</div>
      <div className="ai-providers-layout">
        <div className="col">
          <div className="list-box ai-provider-list">
            {info?.providers.map((p) => (
              <div
                key={p.config.id}
                className={`item${edit?.id === p.config.id && !isNew ? " selected" : ""}`}
                onClick={() => {
                  setEdit(p.config);
                  setIsNew(false);
                  setKey("");
                  setMessage(null);
                }}
              >
                <span className="grow ellipsis">
                  {p.config.display_name}
                  <span className="muted small"> · {p.config.model}</span>
                </span>
                {p.is_default && <span className="pv-tag">{t("ai.providers.default")}</span>}
              </div>
            ))}
            {info && info.providers.length === 0 && <div className="item muted">{t("ai.providers.none")}</div>}
          </div>
          <div className="row">
            <select className="grow" value={preset} onChange={(e) => setPreset(e.target.value)} aria-label={t("ai.providers.kind")}>
              {PROVIDER_PRESETS.map((p) => (
                <option key={p.key} value={p.key}>
                  {p.label}
                </option>
              ))}
            </select>
            <button onClick={add}>{t("ai.providers.add")}</button>
          </div>
        </div>
        {edit ? (
          <div className="form-grid ai-provider-form">
            <label>{t("ai.providers.name")}</label>
            <input type="text" value={edit.display_name} onChange={(e) => set({ display_name: e.target.value })} />
            <label>{t("ai.providers.id")}</label>
            <input type="text" className="mono" value={edit.id} disabled={!isNew} onChange={(e) => set({ id: e.target.value })} />
            <label>{t("ai.providers.model")}</label>
            <>
              <input
                type="text"
                className="mono"
                list="ai-model-suggestions"
                value={edit.model}
                placeholder={t("ai.providers.model_placeholder")}
                onChange={(e) => set({ model: e.target.value })}
              />
              <datalist id="ai-model-suggestions">
                {models.map((m) => (
                  <option key={m} value={m} />
                ))}
              </datalist>
            </>
            {edit.kind === "open_ai_compatible" && (
              <>
                <label>{t("ai.providers.base_url")}</label>
                <input type="text" className="mono" value={edit.base_url ?? ""} onChange={(e) => set({ base_url: e.target.value })} />
              </>
            )}
            <label>{t("ai.providers.key")}</label>
            <div className="row">
              <input
                type="password"
                className="grow"
                autoComplete="off"
                value={key}
                disabled={!info?.keychain}
                placeholder={
                  !info?.keychain
                    ? t("ai.providers.no_keychain")
                    : keySource === "keychain"
                      ? t("ai.providers.key_stored")
                      : keySource === "environment"
                        ? t("ai.providers.key_from_env", { name: edit.key_env ?? "" })
                        : t("ai.providers.key_placeholder")
                }
                onChange={(e) => setKey(e.target.value)}
              />
              {keySource === "keychain" && (
                <button className="text small-btn" onClick={() => void forgetKey()}>
                  {t("ai.providers.forget_key")}
                </button>
              )}
            </div>
            <label>{t("ai.providers.key_env")}</label>
            <input
              type="text"
              className="mono"
              value={edit.key_env ?? ""}
              placeholder="ANTHROPIC_API_KEY"
              onChange={(e) => set({ key_env: e.target.value || null })}
            />
            {edit.kind === "anthropic" && (
              <>
                <label>{t("ai.providers.effort")}</label>
                <select value={edit.effort} onChange={(e) => set({ effort: e.target.value as Effort })}>
                  {EFFORTS.map((e) => (
                    <option key={e} value={e}>
                      {t(`ai.effort.${e}`)}
                    </option>
                  ))}
                </select>
                <span />
                <label className="row">
                  <input type="checkbox" checked={edit.server_fallback} onChange={(e) => set({ server_fallback: e.target.checked })} />
                  {t("ai.providers.fallback")}
                </label>
              </>
            )}
            <span />
            <div className="row">
              <button className="primary" disabled={busy || !!problem} onClick={() => void save(true)}>
                {t("ai.providers.save_test")}
              </button>
              <button disabled={busy || !!problem} onClick={() => void save(false)}>
                {t("btn.save")}
              </button>
              {!isNew && !view?.is_default && (
                <button className="text" disabled={busy} onClick={() => void makeDefault()}>
                  {t("ai.providers.make_default")}
                </button>
              )}
              <span className="spacer" />
              {!isNew && (
                <button className="danger" disabled={busy} onClick={() => void remove()}>
                  {t("btn.delete")}
                </button>
              )}
            </div>
            <span />
            <div className={message ? (message.ok ? "muted" : "error-text") : "pv-hint"}>
              {message ? message.text : problem ? t(problem) : busy ? t("status.loading") : t("ai.providers.privacy")}
            </div>
          </div>
        ) : (
          <div className="pv-hint ai-provider-form">{t("ai.providers.pick")}</div>
        )}
      </div>
    </Dialog>
  );
}

export function openAiProviders(ui: Ui): Promise<void> {
  return ui.showDialog<void>((close) => <AiProvidersDialog ui={ui} close={() => close(undefined)} />, undefined);
}
