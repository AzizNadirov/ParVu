"""
Query toolbar widget for ParVu.
"""
from __future__ import annotations

from PyQt6.QtWidgets import QWidget, QHBoxLayout, QPushButton
from PyQt6.QtCore import pyqtSignal


class QueryToolbar(QWidget):
    """Execute, reset, and table info buttons."""

    execute_clicked = pyqtSignal()
    reset_clicked = pyqtSignal()
    info_clicked = pyqtSignal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        self._execute_btn = QPushButton("Execute Query")
        self._execute_btn.clicked.connect(self.execute_clicked.emit)
        self._execute_btn.setEnabled(False)
        layout.addWidget(self._execute_btn)

        self._reset_btn = QPushButton("Reset Query")
        self._reset_btn.clicked.connect(self.reset_clicked.emit)
        self._reset_btn.setEnabled(False)
        layout.addWidget(self._reset_btn)

        self._info_btn = QPushButton("Table Info")
        self._info_btn.clicked.connect(self.info_clicked.emit)
        self._info_btn.setEnabled(False)
        layout.addWidget(self._info_btn)

        layout.addStretch()

    def set_enabled(self, enabled: bool) -> None:
        self._execute_btn.setEnabled(enabled)
        self._reset_btn.setEnabled(enabled)
        self._info_btn.setEnabled(enabled)
