"""
Pagination bar widget for ParVu.
"""
from __future__ import annotations

from PyQt6.QtWidgets import QWidget, QHBoxLayout, QPushButton, QLabel
from PyQt6.QtCore import pyqtSignal, Qt
from PyQt6.QtGui import QFont


class PaginationBar(QWidget):
    """Pagination controls widget."""

    prev_clicked = pyqtSignal()
    next_clicked = pyqtSignal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        self._prev_btn = QPushButton("◀ Previous")
        self._prev_btn.clicked.connect(self.prev_clicked.emit)
        self._prev_btn.setEnabled(False)
        layout.addWidget(self._prev_btn)

        self._page_label = QLabel("Page: -")
        self._page_label.setFont(QFont("Arial", 10, QFont.Weight.Bold))
        self._page_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(self._page_label, stretch=1)

        self._next_btn = QPushButton("Next ▶")
        self._next_btn.clicked.connect(self.next_clicked.emit)
        self._next_btn.setEnabled(False)
        layout.addWidget(self._next_btn)

    def update_state(self, current_page: int, total_pages: int, total_rows: int) -> None:
        """Update pagination controls."""
        self._page_label.setText(
            f"Page {current_page} of {total_pages:,} ({total_rows:,} rows)"
        )
        self._prev_btn.setEnabled(current_page > 1)
        self._next_btn.setEnabled(current_page < total_pages)
