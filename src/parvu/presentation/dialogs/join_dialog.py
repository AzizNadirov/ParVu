"""
Join Dialog — configure a JOIN between two tables.
"""
from __future__ import annotations

from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QLabel, QComboBox,
    QPushButton, QMessageBox,
)


class JoinDialog(QDialog):
    """Dialog for configuring a SQL JOIN between two tabs."""

    JOIN_TYPES = ["INNER", "LEFT", "RIGHT", "FULL OUTER"]

    def __init__(
        self,
        left_name: str,
        left_columns: list[str],
        right_tables: list[tuple[str, list[str]]],
        parent=None,
    ):
        super().__init__(parent)
        self._left_name = left_name
        self._left_columns = left_columns
        self._right_tables = right_tables
        self.setWindowTitle("Join Tables")
        self.setMinimumWidth(400)
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setSpacing(10)

        # Left table (read-only)
        layout.addWidget(QLabel(f"Left table: <b>{self._left_name}</b>"))

        # Right table
        right_row = QHBoxLayout()
        right_row.addWidget(QLabel("Right table:"))
        self._right_combo = QComboBox()
        for name, _ in self._right_tables:
            self._right_combo.addItem(name)
        self._right_combo.currentIndexChanged.connect(self._on_right_changed)
        right_row.addWidget(self._right_combo)
        layout.addLayout(right_row)

        # Join type
        type_row = QHBoxLayout()
        type_row.addWidget(QLabel("Join type:"))
        self._type_combo = QComboBox()
        self._type_combo.addItems(self.JOIN_TYPES)
        type_row.addWidget(self._type_combo)
        layout.addLayout(type_row)

        # Left key
        left_key_row = QHBoxLayout()
        left_key_row.addWidget(QLabel("Left key:"))
        self._left_key = QComboBox()
        self._left_key.addItems(self._left_columns)
        left_key_row.addWidget(self._left_key)
        layout.addLayout(left_key_row)

        # Right key
        right_key_row = QHBoxLayout()
        right_key_row.addWidget(QLabel("Right key:"))
        self._right_key = QComboBox()
        right_key_row.addWidget(self._right_key)
        layout.addLayout(right_key_row)

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

        # Populate right key for initial selection
        self._on_right_changed(0)

    def _on_right_changed(self, index: int) -> None:
        """Update right-key dropdown when right table changes."""
        self._right_key.clear()
        if 0 <= index < len(self._right_tables):
            _, cols = self._right_tables[index]
            self._right_key.addItems(cols)

    def _on_ok(self) -> None:
        if self._right_combo.count() == 0:
            QMessageBox.warning(self, "Join", "No right table available.")
            return
        if self._left_key.currentText() == self._right_key.currentText() == "":
            QMessageBox.warning(self, "Join", "Please select key columns.")
            return
        self.accept()

    def get_result(self) -> tuple[str, str, str, str, str] | None:
        """Return (right_table, join_type, left_key, right_key, sql)."""
        if self._right_combo.count() == 0:
            return None
        right = self._right_combo.currentText()
        join_type = self._type_combo.currentText()
        left_key = self._left_key.currentText()
        right_key = self._right_key.currentText()

        sql = (
            f"SELECT * FROM {self._left_name} "
            f"{join_type} JOIN {right} "
            f"ON {self._left_name}.{left_key} = {right}.{right_key}"
        )
        return right, join_type, left_key, right_key, sql
