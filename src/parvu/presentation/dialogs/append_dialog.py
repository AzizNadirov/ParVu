"""
Append Dialog — configure a UNION / UNION ALL between tables.
"""
from __future__ import annotations

from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QLabel, QListWidget,
    QListWidgetItem, QRadioButton, QButtonGroup, QPushButton, QMessageBox,
)
from PyQt6.QtCore import Qt


class AppendDialog(QDialog):
    """Dialog for configuring a UNION of multiple tabs."""

    def __init__(
        self,
        current_name: str,
        other_tables: list[str],
        parent=None,
    ):
        super().__init__(parent)
        self._current_name = current_name
        self._other_tables = other_tables
        self.setWindowTitle("Append Tables")
        self.setMinimumWidth(350)
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setSpacing(10)

        layout.addWidget(QLabel(f"Base table: <b>{self._current_name}</b>"))
        layout.addWidget(QLabel("Select tables to append:"))

        self._list = QListWidget()
        for name in self._other_tables:
            item = QListWidgetItem(name)
            item.setCheckState(Qt.CheckState.Unchecked)
            item.setFlags(item.flags() | Qt.ItemFlag.ItemIsUserCheckable)
            self._list.addItem(item)
        layout.addWidget(self._list)

        # Union type
        type_row = QHBoxLayout()
        type_row.addWidget(QLabel("Union mode:"))
        self._union_all = QRadioButton("UNION ALL (keep duplicates)")
        self._union_all.setChecked(True)
        self._union_distinct = QRadioButton("UNION (remove duplicates)")
        type_row.addWidget(self._union_all)
        type_row.addWidget(self._union_distinct)
        layout.addLayout(type_row)

        # Buttons
        btn_row = QHBoxLayout()
        btn_row.addStretch()
        cancel_btn = QPushButton("Cancel")
        cancel_btn.clicked.connect(self.reject)
        btn_row.addWidget(cancel_btn)
        ok_btn = QPushButton("OK")
        ok_btn.clicked.connect(self._on_ok)
        btn_row.addWidget(ok_btn)
        layout.addLayout(btn_row)

    def _on_ok(self) -> None:
        selected = self._selected_tables()
        if not selected:
            QMessageBox.warning(self, "Append", "Please select at least one table.")
            return
        self.accept()

    def _selected_tables(self) -> list[str]:
        """Return list of checked table names."""
        result = []
        for i in range(self._list.count()):
            item = self._list.item(i)
            if item.checkState() == Qt.CheckState.Checked:
                result.append(item.text())
        return result

    def get_result(self) -> tuple[list[str], str, str] | None:
        """Return (selected_tables, union_keyword, sql)."""
        selected = self._selected_tables()
        if not selected:
            return None
        keyword = "UNION" if self._union_distinct.isChecked() else "UNION ALL"
        parts = [f"SELECT * FROM {self._current_name}"]
        for name in selected:
            parts.append(f"SELECT * FROM {name}")
        sql = f"\n{keyword}\n".join(parts)
        return selected, keyword, sql
