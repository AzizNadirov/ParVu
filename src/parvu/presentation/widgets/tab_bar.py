"""
Bottom tab bar for switching between data tables in a ParVu window.

Excel-style sheet tabs with rounded top corners, flat bottom edge,
accent border on active tab, and close buttons.
"""
from __future__ import annotations

from PyQt6.QtWidgets import (
    QWidget, QHBoxLayout, QSizePolicy,
)
from PyQt6.QtCore import Qt, pyqtSignal, QRect
from PyQt6.QtGui import QPainter, QPainterPath, QColor, QFont, QPen
from loguru import logger

from parvu.infrastructure.themes.models import Theme


class AddButton(QWidget):
    """Small + button for adding new tabs."""

    clicked = pyqtSignal()

    def __init__(self, theme: Theme | None = None, parent=None):
        super().__init__(parent)
        self._theme = theme
        self.setFixedSize(30, 24)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setToolTip("Add new tab")

    def set_theme(self, theme: Theme | None) -> None:
        self._theme = theme
        self.update()

    def paintEvent(self, event) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        accent = QColor(self._theme.colors.accent_primary if self._theme else "#2196F3")
        bg = QColor(self._theme.colors.tab_inactive_background if self._theme else "#F3F3F3")
        border = QColor(self._theme.colors.tab_inactive_border if self._theme else "#D4D4D4")

        w, h = self.width(), self.height()
        r = 4
        path = QPainterPath()
        path.addRoundedRect(0, 0, w, h, r, r)
        painter.fillPath(path, bg)
        painter.setPen(QPen(border, 1))
        painter.drawPath(path)

        # Draw + as two lines (more reliable than text at small sizes)
        pen = QPen(accent, 2)
        pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        painter.setPen(pen)
        cx, cy = w // 2, h // 2
        offset = 5
        painter.drawLine(cx, cy - offset, cx, cy + offset)
        painter.drawLine(cx - offset, cy, cx + offset, cy)

    def mousePressEvent(self, event) -> None:
        if event.button() == Qt.MouseButton.LeftButton:
            self.clicked.emit()
        super().mousePressEvent(event)


class TabButton(QWidget):
    """A single sheet tab with Excel-like appearance."""

    clicked = pyqtSignal()
    close_clicked = pyqtSignal()

    def __init__(self, text: str, theme: Theme | None = None, parent=None):
        super().__init__(parent)
        self._text = text
        self._theme = theme
        self._active = False
        self._hovered = False
        self._hover_close = False
        self.setFixedHeight(26)
        self.setMinimumWidth(90)
        self.setMaximumWidth(220)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setToolTip(text)
        self.setSizePolicy(
            QSizePolicy.Policy.Expanding,
            QSizePolicy.Policy.Fixed,
        )

    def set_active(self, active: bool) -> None:
        self._active = active
        self.update()

    def set_theme(self, theme: Theme | None) -> None:
        self._theme = theme
        self.update()

    def _color(self, attr: str, default: str) -> QColor:
        if self._theme:
            return QColor(getattr(self._theme.colors, attr, default))
        return QColor(default)

    def _font_family(self) -> str:
        if self._theme:
            return self._theme.layout.default_font_family
        return "Arial"

    def paintEvent(self, event) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        # Colors
        if self._active:
            bg = self._color("tab_active_background", "#FFFFFF")
            border = self._color("tab_active_border", "#D4D4D4")
            text_color = self._color("tab_text_active", "#333333")
        else:
            bg = self._color("tab_inactive_background", "#F3F3F3")
            border = self._color("tab_inactive_border", "#D4D4D4")
            text_color = self._color("tab_text_inactive", "#666666")

        if self._hovered and not self._active:
            bg = bg.lighter(105)

        # Tab shape: rounded top, flat bottom
        path = QPainterPath()
        r = 6
        w, h = self.width(), self.height()

        path.moveTo(0, h)
        path.lineTo(0, r)
        path.arcTo(0, 0, r * 2, r * 2, 180, -90)
        path.lineTo(w - r, 0)
        path.arcTo(w - r * 2, 0, r * 2, r * 2, 90, -90)
        path.lineTo(w, h)
        path.closeSubpath()

        painter.fillPath(path, bg)
        painter.setPen(QPen(border, 1))
        painter.drawPath(path)

        # Active tab accent line at top
        if self._active:
            accent = self._color("accent_primary", "#217346")
            painter.setPen(QPen(accent, 3))
            painter.drawLine(r, 1, w - r, 1)

        # Draw text
        painter.setPen(text_color)
        font = QFont(self._font_family(), 9)
        if self._active:
            font.setBold(True)
        painter.setFont(font)

        # Text rect (leave room for close button)
        close_space = 22 if (self._active or self._hovered) else 4
        text_rect = QRect(10, 0, max(0, w - 10 - close_space), h)
        elided = painter.fontMetrics().elidedText(
            self._text, Qt.TextElideMode.ElideRight, text_rect.width()
        )
        painter.drawText(
            text_rect, Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter, elided
        )

        # Draw close button
        if self._active or self._hovered:
            close_color = self._color("tab_text_inactive", "#888888")
            if self._hover_close:
                close_color = self._color("tab_close_hover", "#E81123")
            painter.setPen(close_color)
            painter.setFont(QFont("Arial", 13))
            painter.drawText(
                QRect(w - 24, 0, 22, h),
                Qt.AlignmentFlag.AlignCenter,
                "\u00D7",
            )

    def mousePressEvent(self, event) -> None:
        if event.button() == Qt.MouseButton.LeftButton:
            close_rect = QRect(self.width() - 26, 0, 26, self.height())
            if close_rect.contains(event.pos()):
                self.close_clicked.emit()
                return
            self.clicked.emit()
        super().mousePressEvent(event)

    def enterEvent(self, event) -> None:
        self._hovered = True
        self.update()
        super().enterEvent(event)

    def leaveEvent(self, event) -> None:
        self._hovered = False
        self._hover_close = False
        self.update()
        super().leaveEvent(event)

    def mouseMoveEvent(self, event) -> None:
        close_rect = QRect(self.width() - 26, 0, 26, self.height())
        was_hover = self._hover_close
        self._hover_close = close_rect.contains(event.pos())
        if was_hover != self._hover_close:
            self.setCursor(
                Qt.CursorShape.ArrowCursor
                if self._hover_close
                else Qt.CursorShape.PointingHandCursor
            )
            self.update()
        super().mouseMoveEvent(event)


class TabBar(QWidget):
    """Excel-style bottom tab bar for table switching."""

    tab_switched = pyqtSignal(int)      # tab_index
    tab_closed = pyqtSignal(int)        # tab_index
    add_tab_requested = pyqtSignal()

    def __init__(self, parent=None, theme: Theme | None = None):
        super().__init__(parent)
        self.setObjectName("TabBar")
        self._theme = theme
        self._buttons: list[TabButton] = []
        self._active_index: int = -1
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QHBoxLayout(self)
        layout.setContentsMargins(4, 2, 4, 0)
        layout.setSpacing(0)
        layout.setAlignment(Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignBottom)

        # Tab buttons container
        self._tabs_layout = QHBoxLayout()
        self._tabs_layout.setSpacing(2)
        self._tabs_layout.setAlignment(Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignBottom)
        layout.addLayout(self._tabs_layout)

        layout.addStretch(1)

        # Add tab button
        self._add_btn = AddButton(self._theme, self)
        self._add_btn.clicked.connect(self.add_tab_requested.emit)
        layout.addWidget(self._add_btn)

    def set_theme(self, theme: Theme) -> None:
        self._theme = theme
        for btn in self._buttons:
            btn.set_theme(theme)
        self._add_btn.set_theme(theme)

    def set_tabs(self, names: list[str]) -> None:
        """Rebuild tab buttons from names list."""
        while self._tabs_layout.count():
            item = self._tabs_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
        self._buttons.clear()

        for idx, name in enumerate(names):
            btn = TabButton(name, self._theme, self)
            btn.clicked.connect(lambda i=idx: self._on_tab_clicked(i))
            btn.close_clicked.connect(lambda i=idx: self.tab_closed.emit(i))
            self._tabs_layout.addWidget(btn)
            self._buttons.append(btn)

        self._update_active_style()

    def set_active_index(self, index: int) -> None:
        """Highlight the active tab."""
        self._active_index = index
        self._update_active_style()

    def _update_active_style(self) -> None:
        for idx, btn in enumerate(self._buttons):
            btn.set_active(idx == self._active_index)

    def _on_tab_clicked(self, index: int) -> None:
        if index != self._active_index:
            self.tab_switched.emit(index)

    def _show_tab_context_menu(self, index: int, pos) -> None:
        from PyQt6.QtWidgets import QMenu
        from PyQt6.QtGui import QAction
        menu = QMenu(self)
        close_action = QAction("Close tab", self)
        close_action.triggered.connect(lambda: self.tab_closed.emit(index))
        menu.addAction(close_action)
        menu.exec(self._buttons[index].mapToGlobal(pos))
