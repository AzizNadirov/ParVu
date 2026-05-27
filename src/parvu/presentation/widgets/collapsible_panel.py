"""
CollapsiblePanel — reusable widget that wraps content in a toggleable section.

Uses AccordionHeader for a polished accordion look.
"""
from __future__ import annotations

from PyQt6.QtWidgets import QWidget, QVBoxLayout

from parvu.presentation.widgets.accordion_header import AccordionHeader
from parvu.infrastructure.themes.models import Theme


class CollapsiblePanel(QWidget):
    """A panel with an accordion-style header that shows/hides its content."""

    def __init__(
        self,
        title: str,
        expanded: bool = True,
        parent: QWidget | None = None,
    ):
        super().__init__(parent)
        self._title = title
        self._expanded = expanded
        self._theme: Theme | None = None
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(2)

        self._header = AccordionHeader(self._title, self._expanded)
        self._header.toggled.connect(self._toggle)
        layout.addWidget(self._header)

        # Content area — slightly indented for accordion hierarchy
        self._content = QWidget()
        self._content_layout = QVBoxLayout(self._content)
        self._content_layout.setContentsMargins(8, 4, 8, 4)
        self._content_layout.setSpacing(4)

        layout.addWidget(self._content)
        self._content.setVisible(self._expanded)

    def set_theme(self, theme: Theme | None) -> None:
        """Update header colors from the current theme."""
        self._theme = theme
        self._header.set_theme(theme)

    def add_widget(self, widget: QWidget) -> None:
        """Add a widget to the collapsible content area."""
        self._content_layout.addWidget(widget)

    def add_layout(self, layout) -> None:
        """Add a layout to the collapsible content area."""
        self._content_layout.addLayout(layout)

    def _toggle(self) -> None:
        self._expanded = not self._expanded
        self._content.setVisible(self._expanded)
        self._header.set_expanded(self._expanded)

    def set_expanded(self, expanded: bool) -> None:
        """Expand or collapse the panel."""
        if self._expanded != expanded:
            self._toggle()

    def is_expanded(self) -> bool:
        """Return True if the panel is currently expanded."""
        return self._expanded
