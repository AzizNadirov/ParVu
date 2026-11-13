# ParVu Documentation

## For users

| Guide | Contents |
|---|---|
| [QUICK_START.md](QUICK_START.md) | Opening files, querying, editing and exporting |
| [DSL.md](DSL.md) | The expression language |
| [THEMES.md](THEMES.md) | Built-in and custom themes |
| [I18N.md](I18N.md) | Languages and adding a translation |
| [FILE_ASSOCIATIONS.md](FILE_ASSOCIATIONS.md) | Opening files with ParVu from the file manager |
| [CRASH_REPORTING.md](CRASH_REPORTING.md) | Logs and crash reports |

## For developers

| Guide | Contents |
|---|---|
| [BUILDING.md](BUILDING.md) | Requirements, installers, tests, CI |
| [ARCHITECTURE.md](ARCHITECTURE.md) | How the crates and the UI fit together |
| [../AGENTS.md](../AGENTS.md) | Where to find things and how to change them |
| [../plans/RUST_REWRITE_PLAN.md](../plans/RUST_REWRITE_PLAN.md) | The rewrite from Python to Rust, and its status |

## Your files

```
~/.ParVu/
├── settings/settings.json     settings (File → Settings)
├── history/recents.json       recent files
├── themes/*.json              built-in and custom themes
└── logs/                      daily logs and crash reports
```
