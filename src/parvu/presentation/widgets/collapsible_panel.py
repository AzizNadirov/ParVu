"""
CollapsiblePanel — reusable widget that wraps content in a toggleable section.

Used to save vertical space by hiding/showing groups of related widgets.
"""
from __future__ import annotations

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QPushButton,
)
from PyQt6.QtCore import Qt


class CollapsiblePanel(QWidget):
    """A panel with a toggle button header that shows/hides its content."""

    def __init__(
        self,
        title: str,
        expanded: bool = True,
        parent: QWidget | None = None,
    ):
        super().__init__(parent)
        self._title = title
        self._expanded = expanded
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(2)

        # Header row with toggle button
        header = QHBoxLayout()
        header.setSpacing(6)

        arrow = "▼" if self._expanded else "▶"
        self._toggle_btn = QPushButton(f"{arrow} {self._title}")
        self._toggle_btn.setFlat(True)
        self._toggle_btn.setStyleSheet(
            "QPushButton { text-align: left; font-weight: bold; padding: 2px 4px; }"
        )
        self._toggle_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._toggle_btn.clicked.connect(self._toggle)
        header.addWidget(self._toggle_btn)
        header.addStretch()

        layout.addLayout(header)

        # Content area
        self._content = QWidget()
        self._content_layout = QVBoxLayout(self._content)
        self._content_layout.setContentsMargins(0, 0, 0, 0)
        self._content_layout.setSpacing(4)

        layout.addWidget(self._content)
        self._content.setVisible(self._expanded)

    def add_widget(self, widget: QWidget) -> None:
        """Add a widget to the collapsible content area."""
        self._content_layout.addWidget(widget)

    def add_layout(self, layout) -> None:
        """Add a layout to the collapsible content area."""
        self._content_layout.addLayout(layout)

    def _toggle(self) -> None:
        self._expanded = not self._expanded
        self._content.setVisible(self._expanded)
        arrow = "▼" if self._expanded else "▶"
        self._toggle_btn.setText(f"{arrow} {self._title}")

    def set_expanded(self, expanded: bool) -> None:
        """Expand or collapse the panel."""
        if self._expanded != expanded:
            self._toggle()

    def is_expanded(self) -> bool:
        """Return True if the panel is currently expanded."""
        return self._expanded
