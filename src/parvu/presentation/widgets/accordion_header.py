"""
AccordionHeader — styled toggle header for collapsible panels.

Draws a rounded bar with chevron, title, optional left accent bar
when expanded, and a slot for right-side widgets.
"""
from __future__ import annotations

from PyQt6.QtWidgets import QWidget, QHBoxLayout
from PyQt6.QtCore import Qt, pyqtSignal, QRect
from PyQt6.QtGui import QPainter, QPainterPath, QColor, QPen, QFont

from parvu.infrastructure.themes.models import Theme


class AccordionHeader(QWidget):
    """Clickable accordion header with chevron, title, and optional widgets."""

    toggled = pyqtSignal()

    def __init__(
        self,
        title: str,
        expanded: bool = True,
        parent: QWidget | None = None,
    ):
        super().__init__(parent)
        self._title = title
        self._expanded = expanded
        self._hovered = False
        self._theme: Theme | None = None
        self._right_widgets: list[QWidget] = []

        self.setFixedHeight(28)
        self.setCursor(Qt.CursorShape.PointingHandCursor)

        # Layout only manages right-side widgets
        self._layout = QHBoxLayout(self)
        self._layout.setContentsMargins(8, 0, 8, 0)
        self._layout.setSpacing(8)
        self._layout.addStretch(1)

    def add_right_widget(self, widget: QWidget) -> None:
        """Add a widget to the right side of the header."""
        self._right_widgets.append(widget)
        self._layout.addWidget(widget)

    def set_theme(self, theme: Theme | None) -> None:
        """Update colors from the current theme."""
        self._theme = theme
        self.update()

    def set_expanded(self, expanded: bool) -> None:
        """Update the chevron direction."""
        self._expanded = expanded
        self.update()

    def set_title(self, title: str) -> None:
        """Update the header title text."""
        self._title = title
        self.update()

    # --- painting ---

    def paintEvent(self, event) -> None:  # noqa: N802
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        bg = self._color("tab_inactive_background", "#F3F3F3")
        if self._hovered:
            hover = self._color("menu_hover", "#E3F2FD")
            bg = QColor(
                int(bg.red() * 0.7 + hover.red() * 0.3),
                int(bg.green() * 0.7 + hover.green() * 0.3),
                int(bg.blue() * 0.7 + hover.blue() * 0.3),
            )
        border_color = self._color("tab_inactive_border", "#D4D4D4")
        text_color = self._color("tab_text_active", "#333333")
        accent = self._color("accent_primary", "#2196F3")

        w, h = self.width(), self.height()
        r = 4

        # Rounded background + border
        path = QPainterPath()
        path.addRoundedRect(0, 0, w, h, r, r)
        painter.fillPath(path, bg)
        painter.setPen(QPen(border_color, 1))
        painter.drawPath(path)

        # Left accent bar when expanded
        if self._expanded:
            painter.setPen(QPen(accent, 3))
            painter.drawLine(2, r, 2, h - r)

        # Chevron + title
        painter.setPen(text_color)
        font = QFont(self._font_family(), 10)
        font.setBold(True)
        painter.setFont(font)

        arrow = "▼" if self._expanded else "▶"
        painter.drawText(
            QRect(8, 0, 16, h),
            Qt.AlignmentFlag.AlignCenter,
            arrow,
        )

        # Reserve space for right widgets
        right_reserve = 100 if self._right_widgets else 8
        title_rect = QRect(28, 0, max(0, w - 28 - right_reserve), h)
        elided = painter.fontMetrics().elidedText(
            self._title, Qt.TextElideMode.ElideRight, title_rect.width()
        )
        painter.drawText(
            title_rect,
            Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter,
            elided,
        )

    def _color(self, attr: str, default: str) -> QColor:
        if self._theme:
            return QColor(getattr(self._theme.colors, attr, default))
        return QColor(default)

    def _font_family(self) -> str:
        if self._theme:
            return self._theme.layout.default_font_family
        return "Arial"

    # --- mouse handling ---

    def mousePressEvent(self, event) -> None:  # noqa: N802
        if event.button() == Qt.MouseButton.LeftButton:
            self.toggled.emit()
        super().mousePressEvent(event)

    def enterEvent(self, event) -> None:  # noqa: N802
        self._hovered = True
        self.update()
        super().enterEvent(event)

    def leaveEvent(self, event) -> None:  # noqa: N802
        self._hovered = False
        self.update()
        super().leaveEvent(event)
