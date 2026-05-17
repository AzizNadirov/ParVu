"""
Table Info Dialog for ParVu.
"""
from __future__ import annotations

from PyQt6.QtWidgets import QDialog, QVBoxLayout, QTextBrowser


class TableInfoDialog(QDialog):
    """Dialog showing table metadata."""

    def __init__(self, info: dict, columns: list[tuple[str, str]], parent=None):
        super().__init__(parent)
        self._info = info
        self._columns = columns
        self._setup_ui()

    def _setup_ui(self) -> None:
        self.setWindowTitle("Table Information")
        self.resize(600, 500)

        layout = QVBoxLayout(self)

        browser = QTextBrowser()
        browser.setMarkdown(self._build_content())
        layout.addWidget(browser)

    def _build_content(self) -> str:
        md = f"""# Table Information

**File:** {self._info.get('file_path', 'N/A')}

**Total Rows:** {self._info.get('total_rows', 0):,}

**Total Pages:** {self._info.get('total_pages', 0):,}

**Page Size:** {self._info.get('page_size', 0)}

## Columns ({len(self._columns)})

| Column | Type |
|--------|------|
"""
        for col_name, col_type in self._columns:
            md += f"| {col_name} | {col_type} |\n"
        return md
