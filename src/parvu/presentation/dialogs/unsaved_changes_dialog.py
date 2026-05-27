"""
Unsaved Changes Dialog — asks whether to save, discard, or cancel.

Replaces QMessageBox for a consistent, polished look.
"""
from __future__ import annotations

from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
)
from PyQt6.QtCore import Qt


class UnsavedChangesDialog(QDialog):
    """Custom dialog for unsaved cell edits with Save / Discard / Cancel."""

    SAVE = 1
    DISCARD = 2
    CANCEL = 0

    def __init__(self, title: str, message: str, parent=None):
        super().__init__(parent)
        self.setWindowTitle(title)
        self.setWindowFlags(
            Qt.WindowType.Dialog
            | Qt.WindowType.WindowTitleHint
            | Qt.WindowType.WindowCloseButtonHint
        )
        self.setMinimumWidth(360)
        self._result = UnsavedChangesDialog.CANCEL
        self._setup_ui(message)

    def _setup_ui(self, message: str) -> None:
        layout = QVBoxLayout(self)
        layout.setSpacing(16)
        layout.setContentsMargins(20, 20, 20, 20)

        self._label = QLabel(message)
        self._label.setWordWrap(True)
        layout.addWidget(self._label)

        btn_row = QHBoxLayout()
        btn_row.addStretch()

        discard_btn = QPushButton("Close without saving")
        discard_btn.clicked.connect(self._on_discard)
        btn_row.addWidget(discard_btn)

        cancel_btn = QPushButton("Cancel")
        cancel_btn.clicked.connect(self._on_cancel)
        btn_row.addWidget(cancel_btn)

        save_btn = QPushButton("Save")
        save_btn.setDefault(True)
        save_btn.clicked.connect(self._on_save)
        btn_row.addWidget(save_btn)

        layout.addLayout(btn_row)

    def _on_discard(self) -> None:
        self._result = UnsavedChangesDialog.DISCARD
        self.accept()

    def _on_cancel(self) -> None:
        self._result = UnsavedChangesDialog.CANCEL
        self.reject()

    def _on_save(self) -> None:
        self._result = UnsavedChangesDialog.SAVE
        self.accept()

    def get_result(self) -> int:
        return self._result
