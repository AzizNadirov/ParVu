import { useCallback, useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { Ui } from "../lib/ui";
import { api, errorMessage } from "../lib/api";
import type { CustomDefinition, CustomEntry, CustomListing, ImportClash } from "../lib/api";
import { Dialog } from "../components/Dialog";
import { Icon } from "../components/Icon";
import { openCustomEditor } from "./CustomEditorDialog";
import { customCheckText, duplicateDefinition, isUsable, newDefinition, signatureText } from "./logic";
import "./dialogs.css";

const filters = (ui: Ui) => [
  { name: ui.t("custom.file_filter"), extensions: ["json"] },
  { name: ui.t("dialog.common.all_files"), extensions: ["*"] },
];

/** Import a file of custom tools, asking about names that exist. */
export async function importCustomFlow(ui: Ui): Promise<boolean> {
  const t = ui.t;
  const path = await open({ title: t("custom.import_title"), multiple: false, directory: false, filters: filters(ui) });
  if (!path || Array.isArray(path)) return false;
  try {
    const items = await api.customImportPreview(path);
    const decisions: Record<string, ImportClash> = {};
    for (const item of items.filter((i) => i.exists)) {
      const choice = await ui.ask(t("custom.import_clash_title"), t("custom.import_clash", { name: item.name }), [
        { id: "skip", label: t("custom.import.skip") },
        { id: "rename", label: t("custom.import.rename") },
        { id: "replace", label: t("custom.import.replace"), danger: true },
      ]);
      decisions[item.name.toLowerCase()] = choice === "replace" || choice === "rename" ? choice : "skip";
    }
    const r = await api.customImport(path, decisions);
    const lines = [
      r.imported.length ? t("custom.import.imported", { names: r.imported.join(", ") }) : "",
      r.drafts.length ? t("custom.import.drafts", { names: r.drafts.join(", ") }) : "",
      r.skipped.length ? t("custom.import.skipped", { names: r.skipped.join(", ") }) : "",
    ].filter(Boolean);
    await ui.alert(t("custom.import_title"), lines.join("\n\n") || t("custom.import.nothing"), r.drafts.length ? "warning" : "info");
    return true;
  } catch (e) {
    await ui.alert(t("custom.import_failed"), errorMessage(e), "error");
    return false;
  }
}

/** Export `names` (one definition, or several as a bundle). */
export async function exportCustomFlow(ui: Ui, names: string[]): Promise<void> {
  const t = ui.t;
  const defaultPath = names.length === 1 ? `${names[0]}.json` : "custom-tools.parvu-custom.json";
  const path = await save({ title: t("custom.export_title"), defaultPath, filters: filters(ui) });
  if (!path) return;
  try {
    const n = await api.customExport(names, path);
    ui.status(t("custom.exported", { count: n, path }), 5000);
  } catch (e) {
    await ui.alert(t("custom.export_failed"), errorMessage(e), "error");
  }
}

function Badges({ t, e }: { t: Ui["t"]; e: CustomEntry }) {
  const d = e.definition;
  return (
    <span className="row pv-custom-badges">
      {e.error && <span className="pv-tag pv-tag-bad">{t("custom.badge.unreadable")}</span>}
      {e.newer && <span className="pv-tag pv-tag-bad">{t("custom.badge.newer")}</span>}
      {d && <span className="pv-tag">{t(`custom.kind_name.${d.kind}`)}</span>}
      {d?.sample && <span className="pv-tag">{t("custom.badge.sample")}</span>}
      {d?.status === "draft" && <span className="pv-tag pv-tag-warn">{t("custom.badge.draft")}</span>}
      {d?.status === "broken" && <span className="pv-tag pv-tag-bad">{t("custom.badge.broken")}</span>}
      {d?.flags.uses_current_time && <span className="pv-tag">{t("custom.badge.today")}</span>}
    </span>
  );
}

/** Custom tools: the list, with new, edit, duplicate, delete, import,
 * export and restoring the samples. */
function CustomToolsDialog({ ui, close, a }: { ui: Ui; close: () => void; a: { create?: "function" | "operation" } }) {
  const t = ui.t;
  const [listing, setListing] = useState<CustomListing | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [checks, setChecks] = useState<Record<string, boolean | null>>({});
  const [error, setError] = useState("");

  const reload = useCallback(async (select?: string) => {
    try {
      const l = await api.customList();
      setListing(l);
      setSelected((s) => {
        const want = select ?? s;
        const names = l.entries.map((e) => e.definition?.name ?? e.file);
        return want && names.includes(want) ? want : (names[0] ?? null);
      });
    } catch (e) {
      setError(errorMessage(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const entries = listing?.entries ?? [];
  const entry = entries.find((e) => (e.definition?.name ?? e.file) === selected);
  const def = entry?.definition;

  const edit = async (d: CustomDefinition, editing: string | null) => {
    if (!listing) return;
    const saved = await openCustomEditor(ui, { definition: d, editing, typeSets: listing.type_sets, valueTypes: listing.value_types });
    if (saved) await reload(d.name.trim());
  };

  useEffect(() => {
    if (a.create && listing) void edit(newDefinition(a.create), null);
    // Once, when the list first arrives.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [!!listing]);

  const remove = async () => {
    if (!def) return;
    if (!(await ui.confirm(t("custom.delete_title"), t(def.sample ? "custom.delete_sample" : "custom.delete_confirm", { name: def.name })))) return;
    try {
      await api.customDelete(def.name);
      await reload();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const runAll = async () => {
    const out: Record<string, boolean | null> = {};
    for (const e of entries) if (e.definition) out[e.definition.name] = null;
    setChecks({ ...out });
    for (const e of entries) {
      const d = e.definition;
      if (!d) continue;
      try {
        const r = await api.customValidate(d, d.name);
        out[d.name] = !!r.compiled && !r.checks.some((c) => c.status === "fail");
      } catch {
        out[d.name] = false;
      }
      setChecks({ ...out });
    }
  };

  const restore = async () => {
    try {
      const n = await api.customRestoreSamples();
      ui.status(t("custom.samples_restored", { count: n }), 4000);
      await reload();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const toggle = (name: string) =>
    setChecked((s) => {
      const n = new Set(s);
      if (n.has(name)) n.delete(name);
      else n.add(name);
      return n;
    });
  const exportNames = checked.size ? [...checked] : def ? [def.name] : [];

  return (
    <Dialog
      title={t("custom.manager.title")}
      onClose={close}
      className="pv-dialog-wide pv-dialog-tall pv-custom-manager"
      footer={
        <>
          <div className="left">
            <button onClick={async () => (await importCustomFlow(ui)) && (await reload())}>
              <Icon name="importFile" /> {t("btn.import")}
            </button>
            <button disabled={!exportNames.length} onClick={() => void exportCustomFlow(ui, exportNames)}>
              <Icon name="export" /> {checked.size > 1 ? t("custom.manager.export_n", { count: checked.size }) : t("btn.export")}
            </button>
            <button onClick={() => void restore()}>{t("custom.manager.restore")}</button>
          </div>
          <button onClick={close}>{t("btn.close")}</button>
        </>
      }
    >
      <div className="row">
        <button className="primary" onClick={() => void edit(newDefinition("function"), null)}>
          <Icon name="plus" /> {t("custom.manager.new_function")}
        </button>
        <button onClick={() => void edit(newDefinition("operation"), null)}>
          <Icon name="plus" /> {t("custom.manager.new_operation")}
        </button>
        <span className="grow" />
        <button disabled={!def} onClick={() => def && void edit(def, def.name)}>
          <Icon name="edit" /> {t("custom.manager.edit")}
        </button>
        <button
          disabled={!def}
          onClick={() => def && void edit(duplicateDefinition(def, entries.flatMap((e) => (e.definition ? [e.definition.name] : []))), null)}
        >
          <Icon name="duplicate" /> {t("custom.manager.duplicate")}
        </button>
        <button className="danger" disabled={!def} onClick={() => void remove()}>
          <Icon name="delete" /> {t("btn.delete")}
        </button>
        <button disabled={!entries.length} onClick={() => void runAll()} title={t("custom.manager.run_all_hint")}>
          <Icon name="run" /> {t("custom.manager.run_all")}
        </button>
      </div>
      {error && <div className="error-text">{error}</div>}
      <div className="list-box pv-custom-list" role="listbox">
        {entries.length === 0 && <div className="pv-hint pv-custom-empty">{t("custom.manager.empty")}</div>}
        {entries.map((e) => {
          const name = e.definition?.name ?? e.file;
          const result = e.definition ? checks[e.definition.name] : undefined;
          return (
            <div
              key={e.file}
              role="option"
              aria-selected={name === selected}
              className={`item ${name === selected ? "selected" : ""}`}
              onClick={() => setSelected(name)}
              onDoubleClick={() => e.definition && void edit(e.definition, e.definition.name)}
            >
              {e.definition && (
                <input
                  type="checkbox"
                  checked={checked.has(name)}
                  onClick={(ev) => ev.stopPropagation()}
                  onChange={() => toggle(name)}
                  aria-label={t("custom.manager.select")}
                />
              )}
              <div className="col grow pv-custom-item">
                <div className="row">
                  <span className="pv-mono pv-custom-name">{name}</span>
                  {e.definition?.title && <span className="ellipsis">{e.definition.title}</span>}
                  <span className="grow" />
                  {result === null && <span className="pv-hint">…</span>}
                  {result === true && <Icon name="checkCircle" size={16} className="pv-ok" />}
                  {result === false && <Icon name="failCircle" size={16} className="pv-bad" />}
                  <Badges t={t} e={e} />
                </div>
                {e.definition && <span className="pv-mono pv-hint ellipsis">{signatureText(e.definition)}</span>}
                {e.error && <span className="error-text small ellipsis">{e.error}</span>}
              </div>
            </div>
          );
        })}
      </div>
      {def && (
        <div className="col pv-custom-details">
          {def.description && <div>{def.description}</div>}
          <div className="pv-hint">
            {[def.author && t("custom.manager.author", { author: def.author }), def.modified && t("custom.manager.modified", { date: def.modified.replace("T", " ").replace("Z", "") })]
              .filter(Boolean)
              .join(" · ")}
          </div>
          {!isUsable(def) && def.report && (
            <div className="error-text small">
              {def.report.checks
                .filter((c) => c.status === "fail")
                .slice(0, 3)
                .map((c) => customCheckText(t, c.message))
                .join("\n")}
            </div>
          )}
        </div>
      )}
      {listing && <div className="pv-hint pv-selectable">{t("custom.manager.folder", { folder: listing.folder })}</div>}
    </Dialog>
  );
}

export function openCustomTools(ui: Ui, a: { create?: "function" | "operation" } = {}): Promise<void> {
  return ui.showDialog<void>((close) => <CustomToolsDialog ui={ui} close={() => close()} a={a} />, undefined);
}
