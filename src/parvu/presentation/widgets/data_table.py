"""
Custom Data Table View for ParVu.
"""
from __future__ import annotations

from PyQt6.QtWidgets import QTableWidget, QTableWidgetItem, QMenu, QApplication
from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtGui import QFont, QAction

import pandas as pd
from loguru import logger

from parvu.infrastructure.themes.models import Theme


class DataTableView(QTableWidget):
    """Enhanced table widget with column header context menu."""

    sort_requested = pyqtSignal(str, bool)  # column_name, ascending
    unique_values_requested = pyqtSignal(str)  # column_name
    filter_requested = pyqtSignal(str, list)  # column_name, values

    def __init__(self, parent=None, theme: Theme | None = None):
        super().__init__(parent)
        self._current_data = pd.DataFrame()
        self._theme = theme

        font = QFont(
            theme.layout.table_font_family if theme else "Courier",
            theme.layout.table_font_size if theme else 10,
        )
        self.setFont(font)

        if theme:
            self.setShowGrid(theme.layout.show_grid)
            self.setAlternatingRowColors(theme.layout.alternate_row_colors)

        self.setEditTriggers(QTableWidget.EditTrigger.DoubleClicked)
        self.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.customContextMenuRequested.connect(self._show_context_menu)
        self.horizontalHeader().sectionClicked.connect(self._on_header_clicked)

    def load_data(self, df: pd.DataFrame) -> None:
        """Load DataFrame into table."""
        self._current_data = df
        self.setRowCount(len(df))
        self.setColumnCount(len(df.columns))
        self.setHorizontalHeaderLabels(df.columns.tolist())

        for i in range(len(df)):
            for j, col in enumerate(df.columns):
                value = df.iloc[i, j]
                item = QTableWidgetItem(str(value))
                item.setData(Qt.ItemDataRole.UserRole, value)
                self.setItem(i, j, item)

        self.resizeColumnsToContents()

    def _show_context_menu(self, pos) -> None:
        column = self.columnAt(pos.x())
        if column < 0:
            return

        column_name = self.horizontalHeaderItem(column).text()
        menu = QMenu(self)

        copy_name = QAction("Copy Column Name", self)
        copy_name.triggered.connect(lambda: self._copy_column_name(column_name))
        menu.addAction(copy_name)
        menu.addSeparator()

        sort_asc = QAction("Sort Ascending", self)
        sort_asc.triggered.connect(lambda: self.sort_requested.emit(column_name, True))
        menu.addAction(sort_asc)

        sort_desc = QAction("Sort Descending", self)
        sort_desc.triggered.connect(lambda: self.sort_requested.emit(column_name, False))
        menu.addAction(sort_desc)
        menu.addSeparator()

        copy_tuple = QAction("Copy Values as Tuple", self)
        copy_tuple.triggered.connect(lambda: self._copy_column_as_tuple(column))
        menu.addAction(copy_tuple)

        unique = QAction("Show Unique Values...", self)
        unique.triggered.connect(lambda: self.unique_values_requested.emit(column_name))
        menu.addAction(unique)

        menu.exec(self.mapToGlobal(pos))

    def _on_header_clicked(self, logical_index: int) -> None:
        column_name = self.horizontalHeaderItem(logical_index).text()
        self.sort_requested.emit(column_name, True)

    def _copy_column_name(self, column_name: str) -> None:
        QApplication.clipboard().setText(column_name)
        logger.info(f"Copied column name: {column_name}")

    def _copy_column_as_tuple(self, column: int) -> None:
        if self._current_data.empty:
            return
        column_name = self.horizontalHeaderItem(column).text()
        values = self._current_data[column_name].tolist()
        tuple_str = "(" + ", ".join(repr(v) for v in values) + ")"
        QApplication.clipboard().setText(tuple_str)
        logger.info(f"Copied {len(values)} values as tuple")

    def apply_theme(self, theme: Theme) -> None:
        self._theme = theme
        font = QFont(theme.layout.table_font_family, theme.layout.table_font_size)
        self.setFont(font)
        self.setShowGrid(theme.layout.show_grid)
        self.setAlternatingRowColors(theme.layout.alternate_row_colors)
