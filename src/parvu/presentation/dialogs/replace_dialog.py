"""
Replace Dialog — configure string replacement on a column.
"""
from __future__ import annotations

from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QLabel,
    QLineEdit, QComboBox, QCheckBox, QPushButton,
)
from PyQt6.QtCore import Qt


class ReplaceDialog(QDialog):
    """Dialog for configuring REPLACE on a column."""

    def __init__(
        self,
        table_name: str,
        columns: list[str],
        selected_column: str | None = None,
        parent=None,
    ):
        super().__init__(parent)
        self._table_name = table_name
        self._columns = columns
        self._selected_column = selected_column
        self.setWindowTitle("Replace Values")
        self.setMinimumWidth(380)
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setSpacing(10)

        layout.addWidget(QLabel(f"Table: <b>{self._table_name}</b>"))

        # Column selection
        col_row = QHBoxLayout()
        col_row.addWidget(QLabel("Column:"))
        self._col_combo = QComboBox()
        self._col_combo.addItems(self._columns)
        if self._selected_column and self._selected_column in self._columns:
            self._col_combo.setCurrentText(self._selected_column)
        if self._selected_column:
            self._col_combo.setEnabled(False)
        col_row.addWidget(self._col_combo)
        layout.addLayout(col_row)

        # Pattern
        pat_row = QHBoxLayout()
        pat_row.addWidget(QLabel("Find:"))
        self._pattern = QLineEdit()
        self._pattern.setPlaceholderText("Text to find")
        pat_row.addWidget(self._pattern)
        layout.addLayout(pat_row)

        # Replacement
        rep_row = QHBoxLayout()
        rep_row.addWidget(QLabel("Replace with:"))
        self._replacement = QLineEdit()
        self._replacement.setPlaceholderText("Replacement text")
        rep_row.addWidget(self._replacement)
        layout.addLayout(rep_row)

        # Output column name
        out_row = QHBoxLayout()
        out_row.addWidget(QLabel("Output column:"))
        self._output = QLineEdit()
        default_out = f"{self._col_combo.currentText()}_replaced"
        self._output.setText(default_out)
        self._col_combo.currentTextChanged.connect(self._update_default_output)
        out_row.addWidget(self._output)
        layout.addLayout(out_row)

        # Options
        self._case_sensitive = QCheckBox("Case sensitive")
        self._case_sensitive.setChecked(True)
        layout.addWidget(self._case_sensitive)

        self._regex = QCheckBox("Regular expression")
        self._regex.setChecked(False)
        layout.addWidget(self._regex)

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

    def _update_default_output(self, text: str) -> None:
        self._output.setText(f"{text}_replaced")

    def _on_ok(self) -> None:
        if not self._pattern.text():
            from PyQt6.QtWidgets import QMessageBox
            QMessageBox.warning(self, "Replace Values", "Please enter a search pattern.")
            return
        if not self._output.text().strip():
            from PyQt6.QtWidgets import QMessageBox
            QMessageBox.warning(self, "Replace Values", "Please enter an output column name.")
            return
        self.accept()

    def get_result(self) -> tuple[str, str, str, str, bool, bool] | None:
        """Return (column, pattern, replacement, output_col, case_sensitive, regex) or None."""
        if self.result() != QDialog.DialogCode.Accepted:
            return None
        return (
            self._col_combo.currentText(),
            self._pattern.text(),
            self._replacement.text(),
            self._output.text().strip(),
            self._case_sensitive.isChecked(),
            self._regex.isChecked(),
        )
