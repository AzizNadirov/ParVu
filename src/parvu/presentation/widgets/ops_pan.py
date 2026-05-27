"""
Operations Panel (OPSPan) for data transformations.

Inspired by Power BI Power Query — provides buttons for common
table operations that are translated into DuckDB SQL queries.
"""
from __future__ import annotations

from PyQt6.QtWidgets import QWidget, QHBoxLayout, QPushButton
from PyQt6.QtCore import pyqtSignal


class OPSPan(QWidget):
    """Toolbar-style panel with row-level and multi-column operations.

    Column-scoped operations (Change Type, Remove, Rename, Duplicate)
    have moved to the column header context menu.
    """

    add_column_requested = pyqtSignal()
    math_op_requested = pyqtSignal()
    join_requested = pyqtSignal()
    append_requested = pyqtSignal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QHBoxLayout(self)
        layout.setContentsMargins(4, 4, 4, 4)
        layout.setSpacing(8)

        self._add_col_btn = QPushButton("➕ Add Column")
        self._add_col_btn.setToolTip("Add a new column with a SQL expression")
        self._add_col_btn.clicked.connect(self.add_column_requested.emit)
        layout.addWidget(self._add_col_btn)

        self._math_btn = QPushButton("🧮 Math Operation")
        self._math_btn.setToolTip("Create a new column from a math expression")
        self._math_btn.clicked.connect(self.math_op_requested.emit)
        layout.addWidget(self._math_btn)

        self._join_btn = QPushButton("🔗 Join")
        self._join_btn.setToolTip("Join with another table")
        self._join_btn.clicked.connect(self.join_requested.emit)
        layout.addWidget(self._join_btn)

        self._append_btn = QPushButton("⬇️ Append")
        self._append_btn.setToolTip("Append rows from another table")
        self._append_btn.clicked.connect(self.append_requested.emit)
        layout.addWidget(self._append_btn)

        layout.addStretch(1)
