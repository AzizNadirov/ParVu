"""
Applied Steps Panel — shows the history of transforms for the current tab.

Collapsible accordion to save vertical space. Shows step count in the header.
"""
from __future__ import annotations

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QListWidget, QListWidgetItem, QPushButton, QSizePolicy,
)
from PyQt6.QtCore import Qt, pyqtSignal

from parvu.presentation.widgets.accordion_header import AccordionHeader
from parvu.infrastructure.themes.models import Theme


class AppliedStepsPanel(QWidget):
    """Collapsible list of applied transformation steps with undo support."""

    undo_requested = pyqtSignal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self._expanded = False
        self._theme: Theme | None = None
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(2)

        # Accordion header
        self._header = AccordionHeader("Applied Steps (0)", self._expanded)
        self._header.toggled.connect(self._toggle)

        # Undo button inside the header
        self._undo_btn = QPushButton("↩ Undo")
        self._undo_btn.setEnabled(False)
        self._undo_btn.setToolTip("Undo the last applied step")
        self._undo_btn.setStyleSheet("QPushButton { padding: 2px 8px; }")
        self._undo_btn.clicked.connect(self.undo_requested.emit)
        self._header.add_right_widget(self._undo_btn)

        layout.addWidget(self._header)

        # Collapsible content: scrollable step list
        self._content = QWidget()
        self._content.setMaximumHeight(100)
        self._content.setSizePolicy(
            QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Maximum
        )
        content_layout = QVBoxLayout(self._content)
        content_layout.setContentsMargins(8, 0, 8, 0)
        content_layout.setSpacing(2)

        self._list = QListWidget()
        self._list.setMaximumHeight(100)
        self._list.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        content_layout.addWidget(self._list)

        layout.addWidget(self._content)
        self._content.setVisible(False)

    def set_theme(self, theme: Theme | None) -> None:
        """Update header colors from the current theme."""
        self._theme = theme
        self._header.set_theme(theme)

    def _toggle(self) -> None:
        self._expanded = not self._expanded
        self._content.setVisible(self._expanded)
        count = self._list.count()
        self._header.set_title(f"Applied Steps ({count})")
        self._header.set_expanded(self._expanded)

    def set_steps(self, steps: list[str]) -> None:
        """Replace the entire step list."""
        self._list.clear()
        for i, step in enumerate(steps, start=1):
            item = QListWidgetItem(f"{i}. {step}")
            item.setFlags(Qt.ItemFlag.ItemIsEnabled | Qt.ItemFlag.ItemIsSelectable)
            self._list.addItem(item)
        self._undo_btn.setEnabled(bool(steps))
        count = len(steps)
        self._header.set_title(f"Applied Steps ({count})")
        self._header.set_expanded(self._expanded)

    def clear(self) -> None:
        """Remove all steps and collapse."""
        self._list.clear()
        self._undo_btn.setEnabled(False)
        self._header.set_title("Applied Steps (0)")
        if self._expanded:
            self._toggle()

    def set_undo_enabled(self, enabled: bool) -> None:
        """Enable or disable the undo button independently of step count."""
        self._undo_btn.setEnabled(enabled)
