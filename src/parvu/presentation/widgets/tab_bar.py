"""
Bottom tab bar for switching between data tables in a ParVu window.
"""
from __future__ import annotations

from PyQt6.QtWidgets import (
    QWidget, QHBoxLayout, QPushButton, QMenu, QApplication,
)
from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtGui import QFont, QAction
from loguru import logger


class TabBar(QWidget):
    """Excel-style bottom tab bar for table switching."""

    tab_switched = pyqtSignal(int)      # tab_index
    tab_closed = pyqtSignal(int)        # tab_index
    add_tab_requested = pyqtSignal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self._buttons: list[QPushButton] = []
        self._active_index: int = -1
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QHBoxLayout(self)
        layout.setContentsMargins(4, 2, 4, 2)
        layout.setSpacing(2)
        layout.setAlignment(Qt.AlignmentFlag.AlignLeft)

        # Tab buttons container
        self._tabs_layout = QHBoxLayout()
        self._tabs_layout.setSpacing(2)
        self._tabs_layout.setAlignment(Qt.AlignmentFlag.AlignLeft)
        layout.addLayout(self._tabs_layout)

        layout.addStretch(1)

        # Add tab button
        self._add_btn = QPushButton("+")
        self._add_btn.setFixedSize(28, 24)
        self._add_btn.setToolTip("Add new tab")
        self._add_btn.clicked.connect(self.add_tab_requested.emit)
        layout.addWidget(self._add_btn)

    def set_tabs(self, names: list[str]) -> None:
        """Rebuild tab buttons from names list."""
        # Clear existing
        while self._tabs_layout.count():
            item = self._tabs_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
        self._buttons.clear()

        for idx, name in enumerate(names):
            btn = QPushButton(name)
            btn.setCheckable(True)
            btn.setFixedHeight(24)
            btn.setFont(QFont("Arial", 9))
            btn.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
            btn.customContextMenuRequested.connect(lambda pos, i=idx: self._show_tab_context_menu(i, pos))
            btn.clicked.connect(lambda checked, i=idx: self._on_tab_clicked(i))
            self._tabs_layout.addWidget(btn)
            self._buttons.append(btn)

        self._update_active_style()

    def set_active_index(self, index: int) -> None:
        """Highlight the active tab."""
        self._active_index = index
        self._update_active_style()

    def _update_active_style(self) -> None:
        """Update visual state of tab buttons."""
        for idx, btn in enumerate(self._buttons):
            is_active = idx == self._active_index
            btn.setChecked(is_active)
            if is_active:
                btn.setStyleSheet(
                    "QPushButton { background-color: #4a90d9; color: white; "
                    "border: 1px solid #357abd; border-radius: 3px; padding: 2px 10px; }"
                )
            else:
                btn.setStyleSheet(
                    "QPushButton { background-color: #e0e0e0; color: #333; "
                    "border: 1px solid #bbb; border-radius: 3px; padding: 2px 10px; }"
                )

    def _on_tab_clicked(self, index: int) -> None:
        if index != self._active_index:
            self.tab_switched.emit(index)

    def _show_tab_context_menu(self, index: int, pos) -> None:
        menu = QMenu(self)
        close_action = QAction("Close tab", self)
        close_action.triggered.connect(lambda: self.tab_closed.emit(index))
        menu.addAction(close_action)
        menu.exec(self._buttons[index].mapToGlobal(pos))
