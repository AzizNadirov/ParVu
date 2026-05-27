"""
Custom Data Table View for ParVu.
"""
from __future__ import annotations

from PyQt6.QtWidgets import (
    QTableWidget, QTableWidgetItem, QMenu, QApplication,
    QStyledItemDelegate, QWidget
)
from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtGui import QFont, QAction, QColor

import pandas as pd
from loguru import logger

from parvu.infrastructure.themes.models import Theme
from parvu.core.edit_queue import EditQueue, CellEdit


class EditTrackingDelegate(QStyledItemDelegate):
    """Delegate that intercepts cell commits to track edits."""

    def setModelData(self, editor: QWidget, model, index) -> None:
        old_value = model.data(index, Qt.ItemDataRole.DisplayRole)
        super().setModelData(editor, model, index)
        new_value = model.data(index, Qt.ItemDataRole.DisplayRole)
        view = self.parent()
        if isinstance(view, DataTableView):
            view._on_cell_committed(index.row(), index.column(), old_value, new_value)


class DataTableView(QTableWidget):
    """Enhanced table widget with column header context menu and cell editing."""

    sort_requested = pyqtSignal(str, bool)  # column_name, ascending
    unique_values_requested = pyqtSignal(str)  # column_name
    filter_requested = pyqtSignal(str, list)  # column_name, values
    cell_edited = pyqtSignal(int, str, object, object)  # absolute_row, column, old_value, new_value

    # Column transform signals (context menu → MainWindow)
    column_renamed = pyqtSignal(str, str)      # old_name, new_name
    column_removed = pyqtSignal(str)           # column_name
    column_type_changed = pyqtSignal(str, str) # column_name, new_type
    column_duplicated = pyqtSignal(str)        # column_name
    replace_values_requested = pyqtSignal(str) # column_name
    copy_column_tuple_requested = pyqtSignal(str) # column_name

    def __init__(self, parent=None, theme: Theme | None = None):
        super().__init__(parent)
        self._current_data = pd.DataFrame()
        self._theme = theme
        self._edit_queue: EditQueue | None = None
        self._page_offset = 0  # absolute row index of first row in current page
        font = QFont(
            theme.layout.table_font_family if theme else "Courier",
            theme.layout.table_font_size if theme else 10,
        )
        self.setFont(font)

        if theme:
            self.setShowGrid(theme.layout.show_grid)
            self.setAlternatingRowColors(theme.layout.alternate_row_colors)

        self.setEditTriggers(
            QTableWidget.EditTrigger.DoubleClicked | QTableWidget.EditTrigger.EditKeyPressed
        )
        # Column context menu on header right-click
        self.horizontalHeader().setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.horizontalHeader().customContextMenuRequested.connect(self._show_header_context_menu)
        self.horizontalHeader().sectionClicked.connect(self._on_header_clicked)

        # Install edit-tracking delegate
        self._edit_delegate = EditTrackingDelegate(self)
        self.setItemDelegate(self._edit_delegate)


    def set_edit_queue(self, queue: EditQueue | None) -> None:
        """Attach an edit queue to track modifications."""
        self._edit_queue = queue

    def set_page_offset(self, offset: int) -> None:
        """Set the absolute row offset for the current page."""
        self._page_offset = offset

    def load_data(self, df: pd.DataFrame) -> None:
        """Load DataFrame into table."""
        logger.debug(f"DataTable loading {len(df)} rows x {len(df.columns)} cols")
        self._current_data = df
        self.setRowCount(len(df))
        self.setColumnCount(len(df.columns))
        self.setHorizontalHeaderLabels(df.columns.tolist())

        # Temporarily block itemChanged to avoid re-queuing edits while loading
        self.blockSignals(True)
        try:
            for i in range(len(df)):
                for j, col in enumerate(df.columns):
                    value = df.iloc[i, j]
                    item = QTableWidgetItem(str(value))
                    item.setFlags(item.flags() | Qt.ItemFlag.ItemIsEditable)
                    item.setData(Qt.ItemDataRole.UserRole, value)

                    # Check if this cell has a pending edit
                    absolute_row = self._page_offset + i
                    if self._edit_queue is not None and self._edit_queue.has_edit(absolute_row, col):
                        edit = self._edit_queue.get_edit(absolute_row, col)
                        if edit:
                            item.setText(str(edit.new_value))
                            item.setBackground(QColor("#FFF9C4"))  # light yellow for edited cells

                    self.setItem(i, j, item)
        finally:
            self.blockSignals(False)

        self.resizeColumnsToContents()

    def _on_cell_committed(self, row: int, col: int, old_value, new_value) -> None:
        """Called by EditTrackingDelegate when a cell edit is committed."""
        self._process_cell_edit(row, col, old_value, new_value)

    def _process_cell_edit(self, row: int, col: int, old_value, new_value) -> None:
        """Process a cell edit: queue it, highlight, emit signal."""
        if self._edit_queue is None:
            return

        column_name = self.horizontalHeaderItem(col).text() if col >= 0 else ""
        absolute_row = self._page_offset + row

        # Skip if value didn't actually change
        if str(old_value) == str(new_value):
            return

        # Highlight the cell
        item = self.item(row, col)
        if item:
            item.setBackground(QColor("#FFF9C4"))

        # Queue the edit
        edit = CellEdit(
            absolute_row=absolute_row,
            column=column_name,
            old_value=old_value,
            new_value=new_value,
        )
        self._edit_queue.add(edit)
        self.cell_edited.emit(absolute_row, column_name, old_value, new_value)
        logger.info(
            f"Cell edited: row={absolute_row}, col={column_name}, "
            f"{old_value!r} → {new_value!r}"
        )

    def discard_edits(self) -> None:
        """Reload current data without pending edits."""
        self.load_data(self._current_data)

    def _show_header_context_menu(self, pos) -> None:
        column = self.horizontalHeader().logicalIndexAt(pos)
        if column < 0:
            return

        column_name = self.horizontalHeaderItem(column).text()
        logger.debug(f"Header context menu for column '{column_name}'")
        menu = QMenu(self)

        copy_name = QAction("Copy Column Name", self)
        copy_name.triggered.connect(lambda: self._copy_column_name(column_name))
        menu.addAction(copy_name)
        menu.addSeparator()

        # Change Type submenu
        type_menu = menu.addMenu("Change Type")
        for t in ["INTEGER", "BIGINT", "DOUBLE", "VARCHAR", "BOOLEAN", "DATE", "TIMESTAMP"]:
            act = QAction(t, self)
            act.triggered.connect(lambda _c, typ=t, col=column_name: self.column_type_changed.emit(col, typ))
            type_menu.addAction(act)

        rename = QAction("Rename Column...", self)
        rename.triggered.connect(lambda: self._rename_column(column_name))
        menu.addAction(rename)

        duplicate = QAction("Duplicate Column", self)
        duplicate.triggered.connect(lambda: self.column_duplicated.emit(column_name))
        menu.addAction(duplicate)

        remove = QAction("Remove Column", self)
        remove.triggered.connect(lambda: self.column_removed.emit(column_name))
        menu.addAction(remove)
        menu.addSeparator()

        sort_asc = QAction("Sort Ascending", self)
        sort_asc.triggered.connect(lambda: self.sort_requested.emit(column_name, True))
        menu.addAction(sort_asc)

        sort_desc = QAction("Sort Descending", self)
        sort_desc.triggered.connect(lambda: self.sort_requested.emit(column_name, False))
        menu.addAction(sort_desc)
        menu.addSeparator()

        copy_tuple = QAction("Copy Values as Tuple", self)
        copy_tuple.triggered.connect(lambda: self.copy_column_tuple_requested.emit(column_name))
        menu.addAction(copy_tuple)

        unique = QAction("Show Unique Values...", self)
        unique.triggered.connect(lambda: self.unique_values_requested.emit(column_name))
        menu.addAction(unique)
        menu.addSeparator()

        replace = QAction("Replace Values...", self)
        replace.triggered.connect(lambda: self.replace_values_requested.emit(column_name))
        menu.addAction(replace)

        menu.exec(self.horizontalHeader().mapToGlobal(pos))

    def _rename_column(self, column_name: str) -> None:
        from PyQt6.QtWidgets import QInputDialog
        new_name, ok = QInputDialog.getText(self, "Rename Column", "New name:", text=column_name)
        if ok and new_name and new_name != column_name:
            self.column_renamed.emit(column_name, new_name)

    def _on_header_clicked(self, logical_index: int) -> None:
        column_name = self.horizontalHeaderItem(logical_index).text()
        self.sort_requested.emit(column_name, True)

    def _copy_column_name(self, column_name: str) -> None:
        QApplication.clipboard().setText(column_name)
        logger.info(f"Copied column name: {column_name}")

    def get_column_values(self, column_name: str) -> list:
        """Return values for the given column from the current page."""
        if self._current_data.empty or column_name not in self._current_data.columns:
            return []
        return self._current_data[column_name].tolist()

    def _copy_column_as_tuple(self, column: int) -> None:
        if self._current_data.empty:
            return
        column_name = self.horizontalHeaderItem(column).text()
        values = self.get_column_values(column_name)
        tuple_str = "(" + ", ".join(repr(v) for v in values) + ")"
        QApplication.clipboard().setText(tuple_str)
        logger.info(f"Copied {len(values)} values as tuple")

    def apply_theme(self, theme: Theme) -> None:
        self._theme = theme
        font = QFont(theme.layout.table_font_family, theme.layout.table_font_size)
        self.setFont(font)
        self.setShowGrid(theme.layout.show_grid)
        self.setAlternatingRowColors(theme.layout.alternate_row_colors)
