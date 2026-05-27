"""
Applied Steps Panel — shows the history of transforms for the current tab.

Each step is a human-readable description of an operation
(Add Column, Rename, Remove, Sort, etc.) applied to the data.
"""
from __future__ import annotations

from PyQt6.QtWidgets import QWidget, QVBoxLayout, QListWidget, QListWidgetItem, QLabel
from PyQt6.QtCore import Qt


class AppliedStepsPanel(QWidget):
    """Read-only list of applied transformation steps."""

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

    def set_steps(self, steps: list[str]) -> None:
        """Replace the entire step list."""
        self._list.clear()
        for i, step in enumerate(steps, start=1):
            item = QListWidgetItem(f"{i}. {step}")
            item.setFlags(Qt.ItemFlag.ItemIsEnabled | Qt.ItemFlag.ItemIsSelectable)
            self._list.addItem(item)

    def clear(self) -> None:
        """Remove all steps."""
        self._list.clear()
