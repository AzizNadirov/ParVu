"""
Applied Steps Panel — shows the history of transforms for the current tab.

Collapsible to save vertical space. Shows step count in the header.
"""
from __future__ import annotations

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QListWidget, QListWidgetItem,
    QLabel, QPushButton,
)
from PyQt6.QtCore import Qt, pyqtSignal


class AppliedStepsPanel(QWidget):
    """Collapsible list of applied transformation steps with undo support."""

    undo_requested = pyqtSignal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self._expanded = False
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(2)

        # Header row: toggle + count label + undo button
        header = QHBoxLayout()
        header.setSpacing(6)

        self._toggle_btn = QPushButton("▶ Applied Steps (0)")
        self._toggle_btn.setFlat(True)
        self._toggle_btn.setStyleSheet(
            "QPushButton { text-align: left; font-weight: bold; padding: 2px 4px; }"
        )
        self._toggle_btn.clicked.connect(self._toggle)
        header.addWidget(self._toggle_btn)
        header.addStretch()

        self._undo_btn = QPushButton("↩ Undo")
        self._undo_btn.setEnabled(False)
        self._undo_btn.setToolTip("Undo the last applied step")
        self._undo_btn.setStyleSheet("QPushButton { padding: 2px 8px; }")
        self._undo_btn.clicked.connect(self.undo_requested.emit)
        header.addWidget(self._undo_btn)

        layout.addLayout(header)

        # Collapsible content: scrollable step list
        self._content = QWidget()
        content_layout = QVBoxLayout(self._content)
        content_layout.setContentsMargins(0, 0, 0, 0)
        content_layout.setSpacing(2)

        self._list = QListWidget()
        self._list.setMaximumHeight(100)
        self._list.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        content_layout.addWidget(self._list)

        layout.addWidget(self._content)
        self._content.setVisible(False)

    def _toggle(self) -> None:
        self._expanded = not self._expanded
        self._content.setVisible(self._expanded)
        count = self._list.count()
        arrow = "▼" if self._expanded else "▶"
        self._toggle_btn.setText(f"{arrow} Applied Steps ({count})")

    def set_steps(self, steps: list[str]) -> None:
        """Replace the entire step list."""
        self._list.clear()
        for i, step in enumerate(steps, start=1):
            item = QListWidgetItem(f"{i}. {step}")
            item.setFlags(Qt.ItemFlag.ItemIsEnabled | Qt.ItemFlag.ItemIsSelectable)
            self._list.addItem(item)
        self._undo_btn.setEnabled(bool(steps))
        count = len(steps)
        arrow = "▼" if self._expanded else "▶"
        self._toggle_btn.setText(f"{arrow} Applied Steps ({count})")

    def clear(self) -> None:
        """Remove all steps and collapse."""
        self._list.clear()
        self._undo_btn.setEnabled(False)
        if self._expanded:
            self._toggle()
        else:
            self._toggle_btn.setText("▶ Applied Steps (0)")

    def set_undo_enabled(self, enabled: bool) -> None:
        """Enable or disable the undo button independently of step count."""
        self._undo_btn.setEnabled(enabled)
