"""
Applied Steps Panel — shows the history of transforms for the current tab.

Each step is a human-readable description of an operation
(Add Column, Rename, Remove, Sort, etc.) applied to the data.
"""
from __future__ import annotations

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QListWidget, QListWidgetItem,
    QLabel, QPushButton,
)
from PyQt6.QtCore import Qt, pyqtSignal


class AppliedStepsPanel(QWidget):
    """List of applied transformation steps with undo support."""

    undo_requested = pyqtSignal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(4)

        self._label = QLabel("Applied Steps")
        self._label.setStyleSheet("QLabel { font-weight: bold; }")
        layout.addWidget(self._label)

        self._list = QListWidget()
        self._list.setMaximumHeight(120)
        layout.addWidget(self._list)

        btn_row = QHBoxLayout()
        self._undo_btn = QPushButton("↩ Undo")
        self._undo_btn.setEnabled(False)
        self._undo_btn.setToolTip("Undo the last applied step")
        self._undo_btn.clicked.connect(self.undo_requested.emit)
        btn_row.addWidget(self._undo_btn)
        btn_row.addStretch()
        layout.addLayout(btn_row)

    def set_steps(self, steps: list[str]) -> None:
        """Replace the entire step list."""
        self._list.clear()
        for i, step in enumerate(steps, start=1):
            item = QListWidgetItem(f"{i}. {step}")
            item.setFlags(Qt.ItemFlag.ItemIsEnabled | Qt.ItemFlag.ItemIsSelectable)
            self._list.addItem(item)
        self._undo_btn.setEnabled(bool(steps))

    def clear(self) -> None:
        """Remove all steps."""
        self._list.clear()
        self._undo_btn.setEnabled(False)

    def set_undo_enabled(self, enabled: bool) -> None:
        """Enable or disable the undo button independently of step count."""
        self._undo_btn.setEnabled(enabled)
