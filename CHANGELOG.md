# ParVu Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Function Documentation Popup** — Expression editor now shows a tooltip-style popup with function signature, parameters, description, example, and SQL mapping when navigating the completer or clicking a function name.
- **REPLACE function** — New DSL function and method for string replacement with support for literal/regex matching and case-insensitive mode.
  - `REPLACE(text, pattern, with_value, case_sensitive, regex)`
  - Also available as method: `data[name].replace('old', 'new', FALSE, FALSE)`
  - Compiles to DuckDB `REPLACE()` or `REGEXP_REPLACE()` with appropriate flags.
- **Confirm Close dialog** — Warns when closing with applied transforms. Options: Save & Close (exports results), Close anyway, Cancel. Includes "Do not ask again" checkbox with setting persistence.
- **Tab deduplication** — Opening the same file twice creates tabs with `_2`, `_3`, etc. suffixes instead of overwriting.
- **Copy Values as Tuple dialog** — For large tables, shows sampling options: Only this page, First N, Random N (N configurable up to 1000). Optional label prefix (e.g. `my_values = (1, 2, 3)`).
- **Button styling** — Smaller, rounded (8px radius), with 2px margin for a Material-like appearance.

### Changed
- **Assignment compilation** — Overwrites existing columns instead of creating duplicates (e.g. `brand` instead of `brand_1`).
- **Table qualifier stripping** — Math operations and Replace dialog now strip table qualifiers when generating subquery SQL to avoid "table not found" errors.
- **Method call grammar** — Parser now supports method arguments: `data[name].replace('a', 'b')`.
- **DROP_DUPLICATES registry** — First parameter corrected from `columns` to `table`.

### Fixed
- **Expression editor assignment** — Fixed issue where assigning to an existing column created a duplicate column.
- **Replace dialog SQL** — Fixed binder error when using table-qualified column references inside subqueries.

## [0.2.0] - 2025-12-29

### Added - i18n System ✅
- **Internationalization (i18n) support** with 3 complete languages:
  - 🇬🇧 English (en) - Default
  - 🇷🇺 Russian (ru) - Русский - 180+ keys - **FULLY INTEGRATED**
  - 🇦🇿 Azerbaijani (az) - Azərbaycan - 180+ keys - **FULLY INTEGRATED**
- `src/i18n.py` - Core i18n system with Locale and I18n classes
- `src/language_selector.py` - Language selector widgets for settings
- Language preference saved in settings (`current_language` field)
- Translation system with variable formatting support
- Flag emoji display for language identification
- **Full UI integration** - All 4 main UI files translated (~95 strings)

### Added - Documentation
- `docs/I18N.md` - Complete i18n user and developer guide
- `docs/I18N_INTEGRATION_EXAMPLE.md` - Real code integration examples
- `docs/I18N_SUMMARY.md` - Implementation summary and progress
- `docs/README.md` - Documentation index and quick reference
- Organized all documentation into `docs/` directory

### Changed - Themes
- Updated all references in code and documentation
- Theme JSON file now saved as `parvu_black.json`

### Changed - Project Structure
- Moved all documentation to `docs/` directory:
  - `THEMES.md` → `docs/THEMES.md`
  - `MIGRATION.md` → `docs/MIGRATION.md`
  - `PROJECT_STRUCTURE.md` → `docs/PROJECT_STRUCTURE.md`
  - New i18n docs in `docs/`
- Updated README.md with language information and new doc links

### Enhanced - Settings
- Added `current_language` field to Settings model
- Language preference persists across application restarts
- i18n initialized on app startup with saved preference
- Language selector integrated into Settings → General tab
- **Working UI translations** - Change language and see immediate effect after restart

### Enhanced - Documentation
- Updated README.md with i18n features
- Added language FAQ section
- Improved documentation organization
- Added comprehensive guides for developers

## [0.1.0] - Previous Release

### Added - Theme System
- 3 built-in themes (ParVu Light, Excel, ParVu Black)
- Custom theme creation and management
- Import/export themes as JSON
- Full color and layout customization

### Added - Core Features
- Parquet, CSV, JSON file support
- Lazy loading for huge files (8GB+)
- SQL querying with DuckDB
- Pagination and table operations
- Syntax highlighting and auto-completion
- Export to multiple formats

---

## Version Comparison

### What's New in 0.2.0?

**Major Features:**
- 🌍 Multi-language support (English, Russian, Azerbaijani)
- 📚 Comprehensive documentation system
- 🎨 Renamed theme for better branding
- 📁 Organized project structure

**For Users:**
- **Working multi-language UI** - Russian and Azerbaijani fully integrated
- Change interface language in Settings → General tab
- See UI in your language after restart
- Choose from 3 fully translated languages
- Better organized documentation

**For Developers:**
- Easy-to-use translation system
- Extensible i18n architecture
- Complete integration guides
- Ready for UI implementation

---

For detailed documentation, see the [docs/](docs/) directory.
