//! Translations, ported from `infrastructure/i18n`.
//!
//! The catalogs are flat JSON maps exported from the Python locale modules by
//! `scripts/export_locales.py` and embedded at compile time. Lookup order is
//! the current language, then English, then the key itself. Formatting
//! follows Python `str.format` (see [`format`]): with no arguments the text is
//! returned untouched; if formatting fails the unformatted text is returned
//! and a warning logged, as in the Python app.

pub mod format;

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::Serialize;

pub use format::FmtArg;

/// A supported UI language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Language {
    /// ISO 639-1 code, e.g. `"en"`.
    pub code: &'static str,
    /// English name.
    pub name: &'static str,
    /// Name in the language itself.
    pub native_name: &'static str,
    /// Flag emoji.
    pub flag: &'static str,
}

const LANGUAGES: &[Language] = &[
    Language { code: "en", name: "English", native_name: "English", flag: "🇬🇧" },
    Language { code: "ru", name: "Russian", native_name: "Русский", flag: "🇷🇺" },
    Language { code: "az", name: "Azerbaijani", native_name: "Azərbaycan", flag: "🇦🇿" },
];

/// Default (fallback) language code.
pub const DEFAULT_LANGUAGE: &str = "en";

type Catalog = BTreeMap<String, String>;

fn parse_catalog(code: &str, json: &str) -> Catalog {
    serde_json::from_str(json).unwrap_or_else(|e| {
        tracing::error!("Embedded locale {code} is invalid: {e}");
        Catalog::new()
    })
}

static EN: LazyLock<Catalog> = LazyLock::new(|| parse_catalog("en", include_str!("../../locales/en.json")));
static RU: LazyLock<Catalog> = LazyLock::new(|| parse_catalog("ru", include_str!("../../locales/ru.json")));
static AZ: LazyLock<Catalog> = LazyLock::new(|| parse_catalog("az", include_str!("../../locales/az.json")));

fn catalog_for(code: &str) -> Option<&'static Catalog> {
    match code {
        "en" => Some(&EN),
        "ru" => Some(&RU),
        "az" => Some(&AZ),
        _ => None,
    }
}

/// Translator holding the current language.
#[derive(Debug, Clone)]
pub struct I18n {
    current: &'static Language,
}

impl Default for I18n {
    fn default() -> Self {
        Self::new()
    }
}

impl I18n {
    /// English.
    pub fn new() -> Self {
        Self { current: &LANGUAGES[0] }
    }

    /// The given language, or English if it is not supported.
    pub fn with_language(code: &str) -> Self {
        let mut i = Self::new();
        i.set_language(code);
        i
    }

    /// Switch language. Returns `false` (and keeps the current one) if the
    /// code is not supported.
    pub fn set_language(&mut self, code: &str) -> bool {
        match LANGUAGES.iter().find(|l| l.code == code) {
            Some(lang) => {
                self.current = lang;
                tracing::info!("Locale set to: {} ({code})", lang.native_name);
                true
            }
            None => {
                tracing::warn!("Locale not found: {code}");
                false
            }
        }
    }

    /// Current language code.
    pub fn current_language(&self) -> &'static str {
        self.current.code
    }

    /// Current language details.
    pub fn current(&self) -> &'static Language {
        self.current
    }

    /// All supported languages, English first.
    pub fn available_languages() -> &'static [Language] {
        LANGUAGES
    }

    /// The raw key → text map for `lang` (English for unknown codes), for
    /// sending to the UI.
    pub fn catalog(lang: &str) -> &'static BTreeMap<String, String> {
        catalog_for(lang).unwrap_or(&EN)
    }

    /// Raw text for `key`: current language, then English, then the key.
    pub fn text<'a>(&self, key: &'a str) -> &'a str
    where
        'static: 'a,
    {
        catalog_for(self.current.code)
            .and_then(|c| c.get(key))
            .or_else(|| EN.get(key))
            .map(String::as_str)
            .unwrap_or(key)
    }

    /// Translate `key` without arguments (no placeholder substitution, like
    /// Python with no kwargs).
    pub fn t(&self, key: &str) -> String {
        self.text(key).to_owned()
    }

    /// Translate `key` and substitute named arguments, Python
    /// `str.format(**kwargs)` style.
    ///
    /// ```
    /// use parvu_config::I18n;
    /// let i18n = I18n::new();
    /// let s = i18n.tr("status.loaded", &[("filename", "a.csv".into()), ("rows", 1234567.into())]);
    /// assert_eq!(s, "Loaded: a.csv (1,234,567 rows)");
    /// ```
    pub fn tr(&self, key: &str, args: &[(&str, FmtArg)]) -> String {
        let text = self.text(key);
        if args.is_empty() {
            return text.to_owned();
        }
        format::format(text, args).unwrap_or_else(|e| {
            tracing::warn!("Translation formatting error for key '{key}': {e}");
            text.to_owned()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn placeholders(s: &str) -> BTreeSet<format::Field> {
        format::fields(s)
            .unwrap_or_else(|e| panic!("bad template {s:?}: {e}"))
            .into_iter()
            .collect()
    }

    #[test]
    fn catalogs_load() {
        for lang in I18n::available_languages() {
            assert!(I18n::catalog(lang.code).len() > 300, "{} catalog too small", lang.code);
        }
    }

    #[test]
    fn locales_complete_and_placeholders_match() {
        let en = I18n::catalog("en");
        let mut problems = Vec::new();
        for lang in I18n::available_languages().iter().filter(|l| l.code != "en") {
            let cat = I18n::catalog(lang.code);
            for (key, en_text) in en {
                match cat.get(key) {
                    None => problems.push(format!("{}: missing key {key}", lang.code)),
                    Some(text) => {
                        let (a, b) = (placeholders(en_text), placeholders(text));
                        if a != b {
                            problems.push(format!("{}: {key}: placeholders {a:?} vs en {b:?}", lang.code));
                        }
                    }
                }
            }
            for key in cat.keys().filter(|k| !en.contains_key(*k)) {
                problems.push(format!("{}: extra key {key} not in en", lang.code));
            }
        }
        assert!(problems.is_empty(), "locale problems:\n{}", problems.join("\n"));
    }

    #[test]
    fn every_template_formats_with_sample_args() {
        // Every placeholder in every locale must be formattable by our port.
        for lang in I18n::available_languages() {
            let i18n = I18n::with_language(lang.code);
            for (key, text) in I18n::catalog(lang.code) {
                let fields = placeholders(text);
                if fields.is_empty() {
                    continue;
                }
                let args: Vec<(&str, FmtArg)> = fields
                    .iter()
                    .map(|f| {
                        let v: FmtArg = if f.spec.is_empty() { "X".into() } else { 1234.5.into() };
                        (f.name.as_str(), v)
                    })
                    .collect();
                let out = format::format(text, &args)
                    .unwrap_or_else(|e| panic!("{}: {key}: {e}", lang.code));
                assert!(!out.contains('{'), "{}: {key}: {out}", lang.code);
                assert_eq!(i18n.tr(key, &args), out);
            }
        }
    }

    #[test]
    fn lookup_and_fallback() {
        let mut i = I18n::new();
        assert_eq!(i.t("app.name"), "ParVu");
        assert_eq!(i.t("no.such.key"), "no.such.key");
        assert!(i.set_language("ru"));
        assert_eq!(i.current_language(), "ru");
        assert_ne!(i.t("menu.file"), I18n::catalog("en")["menu.file"]);
        assert!(!i.set_language("xx"));
        assert_eq!(i.current_language(), "ru");
        assert_eq!(I18n::with_language("xx").current_language(), "en");
        assert!(std::ptr::eq(I18n::catalog("xx"), I18n::catalog("en")));
    }

    #[test]
    fn formatting_matches_python() {
        let i = I18n::new();
        assert_eq!(
            i.tr("step.cell_edit", &[("column", "price".into()), ("row", 3.into())]),
            "Edit 'price' at row 3"
        );
        // Missing argument: unformatted text, like Python's KeyError path.
        assert_eq!(i.tr("step.cell_edit", &[("column", "price".into())]), "Edit '{column}' at row {row}");
        // No arguments: raw text.
        assert_eq!(i.tr("step.cell_edit", &[]), "Edit '{column}' at row {row}");
    }
}
