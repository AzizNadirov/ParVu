//! The user's custom functions and operations: one JSON file each in
//! `~/.ParVu/custom/` (plans/CUSTOM_TOOLS_PLAN.md §7).
//!
//! A file that cannot be read is listed and skipped; a file made by a newer
//! ParVu is listed and left untouched. The compiled form in a file is a
//! cache: it is used only while its hash and the app and DuckDB versions
//! match, otherwise the definition is validated again ([`CustomStore::revalidate`]).
//! A compiled form read from disk is never trusted on its own (a file
//! placed in the folder by hand could carry any SQL): every usable
//! definition is validated again once per run before it is offered.
//! The sample definitions are written once, on the first start; a marker
//! file records that, so samples the user deletes stay deleted.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use parvu_core::custom::{Bundle, Definition, Library, Report, SCHEMA_VERSION, Status, samples};
use serde::{Deserialize, Serialize};

/// Written once the samples have been seeded.
const SEEDED_MARKER: &str = ".samples_seeded";

/// One file of the folder.
#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<Definition>,
    /// Why the file could not be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Made by a newer ParVu: shown, not used, never rewritten.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub newer: bool,
}

/// What to do with an imported definition whose name exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Clash {
    Replace,
    /// Keep both: the imported one gets a free name (`name_2`).
    Rename,
    Skip,
}

/// What an import did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportResult {
    /// Saved and usable.
    pub imported: Vec<String>,
    /// Saved as drafts: they did not pass validation.
    pub drafts: Vec<String>,
    pub skipped: Vec<String>,
}

pub struct CustomStore {
    dir: PathBuf,
    entries: Vec<Entry>,
    /// Lower-case names whose compiled form this run produced (saved after
    /// validation, or validated again by [`CustomStore::revalidate`]).
    verified: BTreeSet<String>,
}

fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

impl CustomStore {
    /// `~/.ParVu/custom`.
    pub fn default_dir() -> PathBuf {
        parvu_config::AppPaths::default().custom_dir()
    }

    /// Read every `*.json` file of `dir` (missing folder: empty store).
    pub fn open(dir: &Path) -> Self {
        let mut entries = Vec::new();
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "json")).collect())
            .unwrap_or_default();
        files.sort();
        for path in files {
            let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!("custom tool {file}: {e}");
                    entries.push(Entry { file, definition: None, error: Some(e.to_string()), newer: false });
                    continue;
                }
            };
            let schema = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.get("schema").and_then(serde_json::Value::as_u64));
            if schema.is_some_and(|s| s > u64::from(SCHEMA_VERSION)) {
                entries.push(Entry { file, definition: None, error: None, newer: true });
                continue;
            }
            match serde_json::from_str::<Definition>(&text) {
                Ok(d) => entries.push(Entry { file, definition: Some(d), error: None, newer: false }),
                Err(e) => {
                    tracing::warn!("custom tool {file} could not be read: {e}");
                    entries.push(Entry { file, definition: None, error: Some(e.to_string()), newer: false });
                }
            }
        }
        Self { dir: dir.to_path_buf(), entries, verified: BTreeSet::new() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn definitions(&self) -> impl Iterator<Item = &Definition> {
        self.entries.iter().filter_map(|e| e.definition.as_ref())
    }

    pub fn get(&self, name: &str) -> Option<&Definition> {
        self.definitions().find(|d| d.name.eq_ignore_ascii_case(name))
    }

    /// The definitions that can be used now: validated in this run, with
    /// a compiled form that is current for `duckdb` and `app`.
    pub fn library(&self, app: &str, duckdb: &str) -> Library {
        Library::new(self.definitions().filter(|d| self.trusted(d, app, duckdb)).cloned())
    }

    fn trusted(&self, d: &Definition, app: &str, duckdb: &str) -> bool {
        self.verified.contains(&d.name.to_lowercase()) && is_current(d, app, duckdb)
    }

    fn path_of(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.json", name.to_lowercase()))
    }

    /// Write `def` to `<name>.json`. With `replacing`, the definition of that
    /// name is replaced (and its file removed when the name changed).
    /// `def` must come from a validation of this run ([`checked`]): a usable
    /// one is trusted from now on.
    pub fn save(&mut self, mut def: Definition, replacing: Option<&str>) -> std::io::Result<()> {
        let old = replacing.and_then(|n| self.get(n).cloned());
        def.schema = SCHEMA_VERSION;
        def.created = old.as_ref().map(|o| o.created.clone()).filter(|c| !c.is_empty()).unwrap_or_else(now);
        def.modified = now();
        let text = serde_json::to_string_pretty(&def).map_err(std::io::Error::other)?;
        write_atomic(&self.path_of(&def.name), &text)?;
        if let Some(old) = old
            && !old.name.eq_ignore_ascii_case(&def.name)
        {
            self.remove_file(&old.name)?;
        }
        self.entries.retain(|e| e.definition.as_ref().is_none_or(|d| !d.name.eq_ignore_ascii_case(&def.name)));
        let file = format!("{}.json", def.name.to_lowercase());
        self.entries.retain(|e| e.file != file);
        let key = def.name.to_lowercase();
        if def.is_usable() {
            self.verified.insert(key);
        } else {
            self.verified.remove(&key);
        }
        self.entries.push(Entry { file, definition: Some(def), error: None, newer: false });
        self.entries.sort_by(|a, b| a.file.cmp(&b.file));
        Ok(())
    }

    fn remove_file(&mut self, name: &str) -> std::io::Result<()> {
        let path = self.path_of(name);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        self.entries.retain(|e| e.definition.as_ref().is_none_or(|d| !d.name.eq_ignore_ascii_case(name)));
        self.verified.remove(&name.to_lowercase());
        Ok(())
    }

    pub fn delete(&mut self, name: &str) -> std::io::Result<bool> {
        if self.get(name).is_none() {
            return Ok(false);
        }
        self.remove_file(name)?;
        Ok(true)
    }

    /// A name not taken yet: `name`, `name_2`, `name_3`…
    pub fn free_name(&self, name: &str) -> String {
        let taken = |n: &str| self.get(n).is_some();
        if !taken(name) {
            return name.to_string();
        }
        (2..).map(|i| format!("{name}_{i}")).find(|n| !taken(n)).unwrap_or_default()
    }

    /// Write the samples on the first start. Returns how many were written.
    pub fn seed_samples(&mut self, validate: &dyn Fn(&Definition) -> Report) -> usize {
        let marker = self.dir.join(SEEDED_MARKER);
        if marker.exists() {
            return 0;
        }
        let n = self.restore_samples(validate);
        if let Err(e) = write_atomic(&marker, "The sample custom tools were added once; delete this file to add them again.\n") {
            tracing::warn!("could not record that samples were added: {e}");
        }
        n
    }

    /// Add the samples whose names are free (Restore samples).
    pub fn restore_samples(&mut self, validate: &dyn Fn(&Definition) -> Report) -> usize {
        let mut n = 0;
        for s in samples() {
            if self.get(&s.name).is_some() {
                continue;
            }
            let report = validate(&s);
            let def = checked(s, report, false);
            match self.save(def, None) {
                Ok(()) => n += 1,
                Err(e) => tracing::warn!("could not add a sample: {e}"),
            }
        }
        n
    }

    /// Validate again the definitions not validated in this run (all of
    /// them after [`CustomStore::open`]): passing ones get a new compiled
    /// form; failing ones become broken. With `persist`, changed outcomes
    /// are written back. Returns the names that broke.
    pub fn revalidate(&mut self, app: &str, duckdb: &str, persist: bool, validate: &dyn Fn(&Definition) -> Report) -> Vec<String> {
        let stale: Vec<Definition> = self
            .definitions()
            .filter(|d| d.status != Status::Draft && !self.trusted(d, app, duckdb))
            .cloned()
            .collect();
        let mut broken = Vec::new();
        for d in stale {
            let name = d.name.clone();
            let report = validate(&d);
            let passed = report.passed();
            let mut def = d;
            if passed {
                def.status = Status::Ok;
                def.compiled = report.compiled;
                def.report = None;
            } else {
                def.status = Status::Broken;
                def.compiled = None;
                def.report = Some(report);
                broken.push(name.clone());
            }
            if passed {
                self.verified.insert(name.to_lowercase());
            }
            let unchanged = self.get(&name).is_some_and(|old| old.status == def.status && old.compiled == def.compiled);
            if persist && !unchanged {
                let text = serde_json::to_string_pretty(&def).unwrap_or_default();
                if let Err(e) = write_atomic(&self.path_of(&name), &text) {
                    tracing::warn!("custom tool {name}: {e}");
                }
            }
            self.entries.retain(|e| e.definition.as_ref().is_none_or(|x| !x.name.eq_ignore_ascii_case(&name)));
            self.entries.push(Entry { file: format!("{}.json", name.to_lowercase()), definition: Some(def), error: None, newer: false });
        }
        self.entries.sort_by(|a, b| a.file.cmp(&b.file));
        broken
    }

    /// A bundle file with `names` (all definitions when empty).
    pub fn export(&self, names: &[String], path: &Path) -> std::io::Result<usize> {
        let defs: Vec<Definition> = if names.is_empty() {
            self.definitions().cloned().collect()
        } else {
            names.iter().filter_map(|n| self.get(n).cloned()).collect()
        };
        let text = serde_json::to_string_pretty(&Bundle::new(defs.clone())).map_err(std::io::Error::other)?;
        write_atomic(path, &text)?;
        Ok(defs.len())
    }

    /// Import `bundle`: each definition is validated as if typed in.
    /// `decide` says what to do when a name exists. Definitions that fail
    /// validation are kept as drafts with their report.
    pub fn import(
        &mut self,
        bundle: Bundle,
        decide: &BTreeMap<String, Clash>,
        validate: &dyn Fn(&Definition, &CustomStore) -> Report,
    ) -> std::io::Result<ImportResult> {
        let mut out = ImportResult::default();
        for incoming in bundle.definitions {
            let mut def = incoming.for_export();
            def.sample = false;
            let mut replacing = None;
            if self.get(&def.name).is_some() {
                match decide.get(&def.name.to_lowercase()).or_else(|| decide.get(&def.name)).copied().unwrap_or(Clash::Skip) {
                    Clash::Skip => {
                        out.skipped.push(def.name.clone());
                        continue;
                    }
                    Clash::Replace => replacing = Some(def.name.clone()),
                    Clash::Rename => def.name = self.free_name(&def.name.to_lowercase()),
                }
            }
            let report = validate(&def, self);
            let passed = report.passed();
            let name = def.name.clone();
            let def = checked(def, report, true);
            self.save(def, replacing.as_deref())?;
            if passed { out.imported.push(name) } else { out.drafts.push(name) }
        }
        Ok(out)
    }
}

/// The definition with the outcome of its validation: usable when it
/// passed; otherwise a draft (when `draft_on_failure`) or broken, with the
/// report.
pub fn checked(mut def: Definition, report: Report, draft_on_failure: bool) -> Definition {
    if report.passed() {
        def.status = Status::Ok;
        def.compiled = report.compiled;
        def.report = None;
    } else {
        def.status = if draft_on_failure { Status::Draft } else { Status::Broken };
        def.compiled = None;
        def.report = Some(report);
    }
    def
}

/// Usable, with a compiled form made from this source by this app and
/// this DuckDB.
pub fn is_current(d: &Definition, app: &str, duckdb: &str) -> bool {
    d.status == Status::Ok
        && d.compiled.as_ref().is_some_and(|c| c.hash == d.source_hash() && c.app == app && c.duckdb == duckdb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use parvu_core::custom::{ValidateEnv, validate};

    fn check(d: &Definition) -> Report {
        validate(d, &ValidateEnv { name_taken: &|_| None, app_version: "test", expression_sql: None }).unwrap()
    }

    fn duckdb() -> String {
        parvu_core::custom::validate::duckdb_version(&parvu_core::custom::validate::validation_connection().unwrap())
    }

    #[test]
    fn hand_placed_compiled_sql_is_not_trusted() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut s = CustomStore::open(dir.path());
        s.restore_samples(&check);
        let db = duckdb();
        // Someone swaps the compiled SQL of a valid file, keeping its hash
        // and versions.
        let path = dir.path().join("clean_phone.json");
        let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        v["compiled"]["template"] = "(SELECT content FROM read_text('/etc/passwd'))".into();
        std::fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();
        let mut s = CustomStore::open(dir.path());
        assert_eq!(s.library("test", &db).len(), 0, "nothing is offered before it is validated again");
        s.revalidate("test", &db, false, &check);
        let lib = s.library("test", &db);
        assert_eq!(lib.len(), 9);
        let def = lib.get("clean_phone").unwrap();
        let t = &def.compiled.as_ref().unwrap().template;
        assert!(!t.contains("read_text"), "the compiled form is rebuilt from the body: {t}");
        assert!(std::fs::read_to_string(&path).unwrap().contains("read_text"), "persist = false writes nothing");
    }

    #[test]
    fn samples_are_seeded_once() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut s = CustomStore::open(dir.path());
        assert_eq!(s.seed_samples(&check), 9);
        assert_eq!(s.library("test", &duckdb()).len(), 9);
        assert!(s.delete("clean_phone").unwrap());
        let mut again = CustomStore::open(dir.path());
        assert_eq!(again.seed_samples(&check), 0, "deleted samples stay deleted");
        assert!(again.get("clean_phone").is_none());
        assert_eq!(again.restore_samples(&check), 1);
        assert!(again.get("clean_phone").unwrap().sample);
    }

    #[test]
    fn round_trip_corrupt_and_newer_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut s = CustomStore::open(dir.path());
        s.restore_samples(&check);
        std::fs::write(dir.path().join("broken.json"), "{ not json").unwrap();
        std::fs::write(dir.path().join("future.json"), r#"{"schema": 99, "anything": true}"#).unwrap();
        let s = CustomStore::open(dir.path());
        assert_eq!(s.definitions().count(), 9);
        let bad = s.entries().iter().find(|e| e.file == "broken.json").unwrap();
        assert!(bad.error.is_some());
        assert!(s.entries().iter().find(|e| e.file == "future.json").unwrap().newer);
        let d = s.get("top_n_per_group").unwrap();
        assert!(d.is_usable() && !d.created.is_empty());
        // The newer file is never rewritten.
        assert!(std::fs::read_to_string(dir.path().join("future.json")).unwrap().contains("anything"));
    }

    #[test]
    fn stale_compiled_forms_are_rebuilt() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut s = CustomStore::open(dir.path());
        s.restore_samples(&check);
        let db = duckdb();
        assert_eq!(s.library("test", &db).len(), 9);
        // Another app version: nothing is current until revalidated.
        assert_eq!(s.library("2.0", &db).len(), 0);
        let v2 = |d: &Definition| validate(d, &ValidateEnv { name_taken: &|_| None, app_version: "2.0", expression_sql: None }).unwrap();
        // An edited file whose body no longer passes its examples breaks.
        let path = dir.path().join("title_case.json");
        let text = std::fs::read_to_string(&path).unwrap().replace("upper(left(w, 1))", "lower(left(w, 1))");
        std::fs::write(&path, text).unwrap();
        let mut s = CustomStore::open(dir.path());
        let broken = s.revalidate("2.0", &db, true, &v2);
        assert_eq!(broken, ["title_case"]);
        assert_eq!(s.library("2.0", &db).len(), 8);
        let reread = CustomStore::open(dir.path());
        assert_eq!(reread.get("title_case").unwrap().status, Status::Broken);
        assert!(reread.get("title_case").unwrap().report.is_some());
        assert_eq!(reread.library("2.0", &db).len(), 0, "nothing read from disk is trusted before it is validated again");
    }

    #[test]
    fn import_with_a_name_clash() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut s = CustomStore::open(dir.path());
        s.restore_samples(&check);
        let file = dir.path().join("out.parvu-custom.json");
        assert_eq!(s.export(&["clean_phone".into(), "running_total".into()], &file).unwrap(), 2);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains("\"compiled\""), "exports carry no compiled form");
        let bundle = || Bundle::parse(&std::fs::read_to_string(&file).unwrap()).unwrap();
        let v = |d: &Definition, _: &CustomStore| check(d);

        let decide: BTreeMap<String, Clash> = [("clean_phone".to_string(), Clash::Rename), ("running_total".to_string(), Clash::Skip)].into();
        let r = s.import(bundle(), &decide, &v).unwrap();
        assert_eq!(r.imported, ["clean_phone_2"]);
        assert_eq!(r.skipped, ["running_total"]);
        assert!(s.get("clean_phone_2").unwrap().is_usable());
        assert!(!s.get("clean_phone_2").unwrap().sample);

        // A definition that fails validation comes in as a draft.
        let mut b = bundle();
        b.definitions.truncate(1);
        b.definitions[0].name = "phone_broken".into();
        b.definitions[0].body.source = "phone || random()::VARCHAR".into();
        let r = s.import(b, &BTreeMap::new(), &v).unwrap();
        assert_eq!(r.drafts, ["phone_broken"]);
        let d = s.get("phone_broken").unwrap();
        assert_eq!(d.status, Status::Draft);
        assert!(!d.report.as_ref().unwrap().passed());

        // Replace keeps one definition of the name.
        let decide: BTreeMap<String, Clash> = [("clean_phone".to_string(), Clash::Replace)].into();
        let mut b = bundle();
        b.definitions.truncate(1);
        b.definitions[0].title = "Replaced".into();
        s.import(b, &decide, &v).unwrap();
        assert_eq!(s.get("clean_phone").unwrap().title, "Replaced");
        assert_eq!(s.definitions().filter(|d| d.name == "clean_phone").count(), 1);
    }
}
