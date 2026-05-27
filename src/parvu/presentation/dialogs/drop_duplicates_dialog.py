"""
Drop Duplicates Dialog — configure deduplication of rows.
"""
from __future__ import annotations

from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QLabel, QListWidget,
    QListWidgetItem, QComboBox, QPushButton, QMessageBox,
)
from PyQt6.QtCore import Qt


class DropDuplicatesDialog(QDialog):
    """Dialog for configuring row deduplication."""

    def __init__(
        self,
        table_name: str,
        columns: list[str],
        parent=None,
    ):
        super().__init__(parent)
        self._table_name = table_name
        self._columns = columns
        self.setWindowTitle("Drop Duplicates")
        self.setMinimumWidth(350)
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setSpacing(10)

        layout.addWidget(QLabel(f"Table: <b>{self._table_name}</b>"))
        layout.addWidget(QLabel("Select columns to consider for duplicates:"))
        layout.addWidget(QLabel("(If none selected, all columns are used)"))

        self._list = QListWidget()
        for col in self._columns:
            item = QListWidgetItem(col)
            item.setCheckState(Qt.CheckState.Unchecked)
            item.setFlags(item.flags() | Qt.ItemFlag.ItemIsUserCheckable)
            self._list.addItem(item)
        layout.addWidget(self._list)

        # Keep strategy
        keep_row = QHBoxLayout()
        keep_row.addWidget(QLabel("Keep:"))
        self._keep_combo = QComboBox()
        self._keep_combo.addItems(["First", "Last"])
        keep_row.addWidget(self._keep_combo)
        layout.addLayout(keep_row)

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
        selected = self._selected_columns()
        self.accept()

    def _selected_columns(self) -> list[str]:
        """Return list of checked column names."""
        result = []
        for i in range(self._list.count()):
            item = self._list.item(i)
            if item.checkState() == Qt.CheckState.Checked:
                result.append(item.text())
        return result

    def get_result(self) -> tuple[list[str], str] | None:
        """Return (selected_columns, keep_strategy) or None if cancelled."""
        if self.result() != QDialog.DialogCode.Accepted:
            return None
        return self._selected_columns(), self._keep_combo.currentText().lower()
