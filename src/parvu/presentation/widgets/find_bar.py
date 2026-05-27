"""
Find bar — Ctrl+F overlay for searching the current page of a DataTableView.
"""
from __future__ import annotations

from typing import Callable

from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtGui import QKeyEvent
from PyQt6.QtWidgets import (
    QCheckBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QToolButton,
    QWidget,
)


class FindBar(QWidget):
    """A slim search bar that finds matches in the visible page of a table."""

    next_match = pyqtSignal()
    prev_match = pyqtSignal()
    closed = pyqtSignal()
    text_changed = pyqtSignal(str)
    options_changed = pyqtSignal()

    def __init__(
        self,
        translator: Callable[..., str] | None = None,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self.setObjectName("FindBar")
        # Tolerate older callers that don't pass a translator.
        self._t: Callable[..., str] = translator or (lambda k, **kw: k)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(8, 4, 8, 4)
        layout.setSpacing(6)

        self._input = QLineEdit()
        self._input.setPlaceholderText(self._t("findbar.placeholder"))
        self._input.textChanged.connect(self.text_changed.emit)
        self._input.returnPressed.connect(self.next_match.emit)
        layout.addWidget(self._input, stretch=1)

        self._case = QCheckBox("Aa")
        self._case.setToolTip(self._t("findbar.tooltip.case"))
        self._case.stateChanged.connect(self.options_changed.emit)
        layout.addWidget(self._case)

        self._whole = QCheckBox("W")
        self._whole.setToolTip(self._t("findbar.tooltip.whole"))
        self._whole.stateChanged.connect(self.options_changed.emit)
        layout.addWidget(self._whole)

        self._count = QLabel("")
        self._count.setMinimumWidth(80)
        layout.addWidget(self._count)

        prev_btn = QToolButton()
        prev_btn.setText("◀")
        prev_btn.setToolTip(self._t("findbar.tooltip.prev"))
        prev_btn.clicked.connect(self.prev_match.emit)
        layout.addWidget(prev_btn)

        next_btn = QToolButton()
        next_btn.setText("▶")
        next_btn.setToolTip(self._t("findbar.tooltip.next"))
        next_btn.clicked.connect(self.next_match.emit)
        layout.addWidget(next_btn)

        close_btn = QToolButton()
        close_btn.setText("✕")
        close_btn.setToolTip(self._t("findbar.tooltip.close"))
        close_btn.clicked.connect(self.closed.emit)
        layout.addWidget(close_btn)

        self.hide()

    def focus_input(self, preset: str = "") -> None:
        if preset:
            self._input.setText(preset)
        self._input.setFocus()
        self._input.selectAll()

    @property
    def query(self) -> str:
        return self._input.text()

    @property
    def case_sensitive(self) -> bool:
        return self._case.isChecked()

    @property
    def whole_cell(self) -> bool:
        return self._whole.isChecked()

    def set_match_count(self, current: int, total: int) -> None:
        if total == 0:
            self._count.setText(
                self._t("findbar.no_matches") if self._input.text() else ""
            )
        else:
            self._count.setText(
                self._t("findbar.count", current=current, total=total)
            )

    def keyPressEvent(self, event: QKeyEvent) -> None:
        if event.key() == Qt.Key.Key_Escape:
            self.closed.emit()
            return
        if event.key() in (Qt.Key.Key_Return, Qt.Key.Key_Enter):
            if event.modifiers() & Qt.KeyboardModifier.ShiftModifier:
                self.prev_match.emit()
            else:
                self.next_match.emit()
            return
        super().keyPressEvent(event)
