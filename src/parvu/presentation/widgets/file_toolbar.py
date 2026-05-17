"""
File toolbar widget for ParVu.
"""
from __future__ import annotations

from PyQt6.QtWidgets import QWidget, QHBoxLayout, QLineEdit, QPushButton
from PyQt6.QtCore import pyqtSignal


class FileToolbar(QWidget):
    """File path input and browse/load buttons."""

    browse_clicked = pyqtSignal()
    load_clicked = pyqtSignal()
    path_changed = pyqtSignal(str)

    def __init__(self, parent=None):
        super().__init__(parent)
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        self._path_edit = QLineEdit()
        self._path_edit.setPlaceholderText("Select a Parquet, CSV, or JSON file...")
        self._path_edit.textChanged.connect(self.path_changed.emit)
        layout.addWidget(self._path_edit, stretch=3)

        self._browse_btn = QPushButton("Browse & Load...")
        self._browse_btn.clicked.connect(self.browse_clicked.emit)
        layout.addWidget(self._browse_btn)

        self._load_btn = QPushButton("Reload from Path")
        self._load_btn.clicked.connect(self.load_clicked.emit)
        layout.addWidget(self._load_btn)

    @property
    def path(self) -> str:
        return self._path_edit.text().strip()

    def set_path(self, path: str) -> None:
        self._path_edit.setText(path)

    def set_enabled(self, enabled: bool) -> None:
        self._browse_btn.setEnabled(enabled)
        self._load_btn.setEnabled(enabled)
