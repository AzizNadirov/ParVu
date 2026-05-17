"""
About Dialog for ParVu.
"""
from __future__ import annotations

from PyQt6.QtWidgets import QDialog, QVBoxLayout, QPushButton, QTextBrowser
from PyQt6.QtCore import Qt

from parvu.infrastructure.i18n.translator import _Translator


class AboutDialog(QDialog):
    """About / Help dialog."""

    def __init__(self, translator: _Translator, parent=None):
        super().__init__(parent)
        self._t = translator
        self._setup_ui()

    def _setup_ui(self) -> None:
        self.setWindowTitle(self._t("dialog.about"))
        self.resize(700, 650)

        layout = QVBoxLayout(self)

        browser = QTextBrowser()
        browser.setOpenExternalLinks(True)
        browser.setMarkdown(self._build_content())
        layout.addWidget(browser)

        close_btn = QPushButton(self._t("btn.close"))
        close_btn.clicked.connect(self.close)
        layout.addWidget(close_btn)

    def _build_content(self) -> str:
        t = self._t
        return f"""# {t("about.title")}

{t("about.description")}

## {t("about.features")}

- {t("about.feature.lazy")}
- {t("about.feature.sql")}
- {t("about.feature.autocomplete")}
- {t("about.feature.pagination")}
- {t("about.feature.editing")}
- {t("about.feature.operations")}
- {t("about.feature.export")}
- {t("about.feature.themes")}

## {t("about.quick_start")}

{t("about.step1")}
{t("about.step2")}
{t("about.step3")}
{t("about.step3_sub")}
{t("about.step4")}
{t("about.step5")}
{t("about.step6")}
{t("about.step7")}

## {t("about.sql_reference")}

{t("about.sql_description")}

```sql
-- View all data
SELECT * FROM data

-- Filter rows
SELECT * FROM data WHERE age > 25

-- Aggregate data
SELECT category, COUNT(*) as count
FROM data GROUP BY category

-- Multiple value filter
SELECT * FROM data WHERE status IN ('active', 'pending')
```

For complete SQL documentation, visit:
**[DuckDB SQL Query Syntax](https://duckdb.org/docs/stable/sql/query_syntax/select)**

## {t("about.shortcuts")}

- {t("about.shortcuts.tab")}
- {t("about.shortcuts.double")}
- {t("about.shortcuts.right")}

## {t("about.resources")}

- **GitHub Repository**: [https://github.com/AzizNadirov/ParVu](https://github.com/AzizNadirov/ParVu)
- **DuckDB Documentation**: [https://duckdb.org/docs/stable/sql/query_syntax/select](https://duckdb.org/docs/stable/sql/query_syntax/select)
- **Report Issues**: [GitHub Issues](https://github.com/AzizNadirov/ParVu/issues)

---

**{t("about.version")}**: {t("about.version_number")}
**{t("about.built_with")}**: {t("about.built_with_text")}
**{t("about.license")}**: {t("about.license_text")}
"""
