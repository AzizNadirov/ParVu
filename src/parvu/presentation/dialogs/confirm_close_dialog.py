"""
Confirm Close Dialog — warns about applied transforms on exit.
"""
from __future__ import annotations

from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QLabel,
    QCheckBox, QPushButton,
)
from PyQt6.QtCore import Qt


class ConfirmCloseDialog(QDialog):
    """Dialog shown when closing with applied transforms."""

    SAVE_AND_CLOSE = 1
    CLOSE_ANYWAY = 2
    CANCEL = 0

    def __init__(self, message: str, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Confirm Close")
        self.setMinimumWidth(380)
        self._result = ConfirmCloseDialog.CANCEL
        self._setup_ui(message)

    def _setup_ui(self, message: str) -> None:
        layout = QVBoxLayout(self)
        layout.setSpacing(12)

        self._label = QLabel(message)
        self._label.setWordWrap(True)
        layout.addWidget(self._label)

        self._dont_ask = QCheckBox("Do not ask again")
        layout.addWidget(self._dont_ask)

        btn_row = QHBoxLayout()
        btn_row.addStretch()

        cancel_btn = QPushButton("Cancel")
        cancel_btn.clicked.connect(self._on_cancel)
        btn_row.addWidget(cancel_btn)

        close_btn = QPushButton("Close anyway")
        close_btn.clicked.connect(self._on_close_anyway)
        btn_row.addWidget(close_btn)

        save_btn = QPushButton("Save & Close")
        save_btn.setDefault(True)
        save_btn.clicked.connect(self._on_save_and_close)
        btn_row.addWidget(save_btn)

        layout.addLayout(btn_row)

    def _on_cancel(self) -> None:
        self._result = ConfirmCloseDialog.CANCEL
        self.reject()

    def _on_close_anyway(self) -> None:
        self._result = ConfirmCloseDialog.CLOSE_ANYWAY
        self.accept()

    def _on_save_and_close(self) -> None:
        self._result = ConfirmCloseDialog.SAVE_AND_CLOSE
        self.accept()

    def get_result(self) -> int:
        return self._result

    def dont_ask_again(self) -> bool:
        return self._dont_ask.isChecked()
