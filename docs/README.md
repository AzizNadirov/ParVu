# ParVu Documentation

Welcome to the ParVu documentation! This directory contains comprehensive guides for using and developing ParVu.

## 📚 Documentation Index

### User Guides

- **[THEMES.md](THEMES.md)** - Complete theme system guide
  - Using built-in themes
  - Creating custom themes
  - Importing/exporting themes
  - Color schemes and layouts
  - Theme best practices

- **[I18N.md](I18N.md)** - Internationalization (i18n) guide
  - Available languages
  - Changing language
  - Translation key reference
  - Adding new languages
  - Best practices

- **[DSL.md](DSL.md)** - Expression language guide
  - Writing expressions
  - Assignment syntax (`data[col] = expr`)
  - Built-in functions
  - Drop duplicates
  - Auto-completion

### Developer Guides

- **[../BUILDING.md](../BUILDING.md)** - Complete build guide
  - Prerequisites and setup
  - Building for Linux and Windows
  - Distribution packages
  - File associations
  - Troubleshooting

- **[I18N_INTEGRATION_EXAMPLE.md](I18N_INTEGRATION_EXAMPLE.md)** - i18n integration examples
  - Real code examples from ParVu
  - Before/after comparisons
  - Integration patterns
  - File-by-file checklist
  - Testing procedures

- **[I18N_SUMMARY.md](I18N_SUMMARY.md)** - i18n implementation summary
  - What's implemented
  - Progress tracking
  - Quick start guide
  - Usage examples

- **[PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md)** - Project architecture
  - Directory structure
  - Module organization
  - Dependencies
  - Build system

- **[MIGRATION.md](MIGRATION.md)** - Migration guide
  - Version migration notes
  - Breaking changes
  - Upgrade instructions

## 🎨 Themes

ParVu includes 3 professionally designed themes:

- **ParVu Light** - Default light theme with blue/green accents
- **Excel** - Microsoft Excel-inspired green theme
- **ParVu Black** - Dark theme inspired by Visual Studio Code

See [THEMES.md](THEMES.md) for complete documentation.

## 🌍 Languages

ParVu supports multiple interface languages:

- 🇬🇧 **English** - Default
- 🇷🇺 **Russian** (Русский) - Full translation (180+ keys)
- 🇦🇿 **Azerbaijani** (Azərbaycan) - Full translation (180+ keys)

See [I18N.md](I18N.md) for complete documentation.

## 🚀 Quick Links

### For Users
- [How to change themes](THEMES.md#changing-themes)
- [How to change language](I18N.md#using-i18n-in-code)
- [How to create custom themes](THEMES.md#creating-custom-themes)
- [Theme troubleshooting](THEMES.md#troubleshooting)

### For Developers
- [Building ParVu](../BUILDING.md)
- [Adding i18n to UI code](I18N_INTEGRATION_EXAMPLE.md)
- [Creating new language translations](I18N.md#adding-a-new-language)
- [i18n implementation status](I18N_SUMMARY.md#integration-progress)
- [Project structure overview](PROJECT_STRUCTURE.md)

## 📖 Documentation by Topic

### Theming
| Document | Description |
|----------|-------------|
| [THEMES.md](THEMES.md) | Complete theme system guide |

### Internationalization
| Document | Description |
|----------|-------------|
| [I18N.md](I18N.md) | User and developer guide |
| [I18N_INTEGRATION_EXAMPLE.md](I18N_INTEGRATION_EXAMPLE.md) | Code integration examples |
| [I18N_SUMMARY.md](I18N_SUMMARY.md) | Implementation summary |

### Development
| Document | Description |
|----------|-------------|
| [../BUILDING.md](../BUILDING.md) | Complete build guide |
| [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md) | Project architecture |
| [MIGRATION.md](MIGRATION.md) | Version migration guide |

## 🔧 Configuration Files

ParVu stores configuration in `~/.ParVu/`:

```
~/.ParVu/
├── settings/
│   └── settings.json       # User settings
├── themes/
│   ├── parvu_light.json    # Built-in themes
│   ├── excel.json
│   ├── parvu_black.json
│   └── custom_*.json       # Custom themes
├── history/
│   └── recents.json        # Recent files
└── logs/
    └── parvu_*.log         # Application logs
```

### Settings Configuration

Key settings in `~/.ParVu/settings/settings.json`:

```json
{
  "current_theme": "ParVu Light",
  "current_language": "en",
  "default_data_var_name": "data",
  "result_pagination_rows_per_page": "100",
  "enable_large_dataset_warning": true,
  // ... more settings
}
```

## 🎯 Feature Overview

### Theme System
- 3 built-in themes
- Custom theme creation
- Import/export themes
- Full color customization
- Font and layout control
- Live preview

### Internationalization
- 3 languages (en, ru, az)
- 180+ translation keys
- Easy language switching
- Extensible system
- Flag icon display
- Native language names

### Data Viewing
- Parquet, CSV, JSON support
- Lazy loading for huge files
- Pagination
- SQL querying + Expression language (DSL)
- Syntax highlighting
- Auto-completion
- Applied steps with undo
- Cell editing tracked in steps

## 📝 Contributing

### Adding Documentation

When adding new features, please:

1. Update relevant documentation
2. Add examples if applicable
3. Update this index
4. Keep formatting consistent
5. Include screenshots for UI features

### Documentation Style

- Use Markdown format
- Include code examples
- Add table of contents for long docs
- Use clear headings
- Include troubleshooting sections

## 🐛 Reporting Issues

Found an issue with documentation?

1. Check existing issues on GitHub
2. Search documentation for answers
3. [Open a new issue](https://github.com/AzizNadirov/ParVu/issues)
4. Include:
   - Documentation file name
   - Section/heading
   - What's unclear or incorrect
   - Suggested improvement

## 📞 Support

- **GitHub Issues**: [ParVu Issues](https://github.com/AzizNadirov/ParVu/issues)
- **Telegram**: [@aziz_nadirov](https://t.me/aziz_nadirov)
- **Main README**: [../README.md](../README.md)

## 📅 Version History

### Version 0.2.0 (2025-12-29)
- ✅ Added i18n system with 3 languages (English, Russian, Azerbaijani)
- ✅ Renamed "VS Code Black" theme to "ParVu Black"
- ✅ Created comprehensive documentation (7 docs)
- ✅ Organized all docs into docs/ directory
- ✅ Language preference saved in settings

### Post-0.2.0 Updates
- ✅ Expression language (DSL) with assignment syntax
- ✅ Drop duplicates operation (UI + DSL)
- ✅ Applied steps panel with undo
- ✅ Cell edit tracking and undo
- ✅ Operations menu (Math, Join, Append, Drop Duplicates)
- ✅ Collapsible applied steps panel
- ✅ Dual-mode query editor (SQL / Expression)

### Version 0.1.0
- Theme system implementation (3 built-in themes)
- Core data viewing features
- Project structure documentation
- Migration guides

---

**Last Updated**: 2026-05-26
**ParVu Version**: 0.2.0+
**Documentation Status**: Complete

For the main project README, see [../README.md](../README.md)
