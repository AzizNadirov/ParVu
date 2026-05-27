"""
Custom Data Table View for ParVu.
"""
from __future__ import annotations

from typing import Callable

from PyQt6.QtWidgets import (
    QTableWidget, QTableWidgetItem, QMenu, QApplication,
    QStyledItemDelegate, QWidget, QSizePolicy
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
    drop_null_requested = pyqtSignal(str) # column_name
    column_stats_requested = pyqtSignal(str) # column_name

    def __init__(
        self,
        parent=None,
        theme: Theme | None = None,
        translator: Callable[..., str] | None = None,
    ):
        super().__init__(parent)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self._current_data = pd.DataFrame()
        self._theme = theme
        self._t: Callable[..., str] = translator or (lambda k, **kw: k)
        self._edit_queue: EditQueue | None = None
        self._page_offset = 0  # absolute row index of first row in current page
        self._sort_column: str | None = None
        self._sort_ascending: bool = True
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
        header = self.horizontalHeader()
        header.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        header.customContextMenuRequested.connect(self._show_header_context_menu)
        header.sectionClicked.connect(self._on_header_clicked)
        header.setSortIndicatorShown(True)
        header.setSectionsClickable(True)

        # Cell context menu (Copy as CSV/TSV/Markdown)
        self.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.customContextMenuRequested.connect(self._show_cell_context_menu)

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

        # Re-apply sort indicator after header rebuild
        if self._sort_column is not None and self._sort_column in df.columns:
            col_idx = list(df.columns).index(self._sort_column)
            order = (
                Qt.SortOrder.AscendingOrder if self._sort_ascending
                else Qt.SortOrder.DescendingOrder
            )
            self.horizontalHeader().setSortIndicator(col_idx, order)

    def reset_sort_state(self) -> None:
        """Clear the tracked sort column (called when a new file/tab is opened)."""
        self._sort_column = None
        self._sort_ascending = True
        self.horizontalHeader().setSortIndicator(-1, Qt.SortOrder.AscendingOrder)

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

        copy_name = QAction(self._t("context.copy_column"), self)
        copy_name.triggered.connect(lambda: self._copy_column_name(column_name))
        menu.addAction(copy_name)
        menu.addSeparator()

        # Change Type submenu
        type_menu = menu.addMenu(self._t("context.change_type"))
        for t in ["INTEGER", "BIGINT", "DOUBLE", "VARCHAR", "BOOLEAN", "DATE", "TIMESTAMP"]:
            act = QAction(t, self)
            act.triggered.connect(lambda _c, typ=t, col=column_name: self.column_type_changed.emit(col, typ))
            type_menu.addAction(act)

        rename = QAction(self._t("context.rename_column"), self)
        rename.triggered.connect(lambda: self._rename_column(column_name))
        menu.addAction(rename)

        duplicate = QAction(self._t("context.duplicate_column"), self)
        duplicate.triggered.connect(lambda: self.column_duplicated.emit(column_name))
        menu.addAction(duplicate)

        remove = QAction(self._t("context.remove_column"), self)
        remove.triggered.connect(lambda: self.column_removed.emit(column_name))
        menu.addAction(remove)
        menu.addSeparator()

        sort_asc = QAction(self._t("context.sort_asc"), self)
        sort_asc.triggered.connect(lambda: self._apply_sort(column_name, True))
        menu.addAction(sort_asc)

        sort_desc = QAction(self._t("context.sort_desc"), self)
        sort_desc.triggered.connect(lambda: self._apply_sort(column_name, False))
        menu.addAction(sort_desc)
        menu.addSeparator()

        copy_tuple = QAction(self._t("context.copy_values"), self)
        copy_tuple.triggered.connect(lambda: self.copy_column_tuple_requested.emit(column_name))
        menu.addAction(copy_tuple)

        unique = QAction(self._t("context.unique_values"), self)
        unique.triggered.connect(lambda: self.unique_values_requested.emit(column_name))
        menu.addAction(unique)

        stats = QAction(self._t("context.column_stats"), self)
        stats.triggered.connect(lambda: self.column_stats_requested.emit(column_name))
        menu.addAction(stats)
        menu.addSeparator()

        replace = QAction(self._t("context.replace_values"), self)
        replace.triggered.connect(lambda: self.replace_values_requested.emit(column_name))
        menu.addAction(replace)

        drop_null = QAction(self._t("context.drop_null"), self)
        drop_null.triggered.connect(lambda: self.drop_null_requested.emit(column_name))
        menu.addAction(drop_null)

        menu.exec(self.horizontalHeader().mapToGlobal(pos))

    def _rename_column(self, column_name: str) -> None:
        from PyQt6.QtWidgets import QInputDialog
        new_name, ok = QInputDialog.getText(
            self,
            self._t("context.rename_dialog.title"),
            self._t("context.rename_dialog.prompt"),
            text=column_name,
        )
        if ok and new_name and new_name != column_name:
            self.column_renamed.emit(column_name, new_name)

    def _on_header_clicked(self, logical_index: int) -> None:
        item = self.horizontalHeaderItem(logical_index)
        if item is None:
            return
        column_name = item.text()
        if self._sort_column == column_name:
            ascending = not self._sort_ascending
        else:
            ascending = True
        self._apply_sort(column_name, ascending)

    def _apply_sort(self, column_name: str, ascending: bool) -> None:
        """Update internal sort state, refresh indicator, emit signal."""
        self._sort_column = column_name
        self._sort_ascending = ascending
        col_idx = -1
        for i in range(self.columnCount()):
            it = self.horizontalHeaderItem(i)
            if it and it.text() == column_name:
                col_idx = i
                break
        if col_idx >= 0:
            order = (
                Qt.SortOrder.AscendingOrder if ascending else Qt.SortOrder.DescendingOrder
            )
            self.horizontalHeader().setSortIndicator(col_idx, order)
        self.sort_requested.emit(column_name, ascending)

    def _copy_column_name(self, column_name: str) -> None:
        QApplication.clipboard().setText(column_name)
        logger.info(f"Copied column name: {column_name}")

    def _show_cell_context_menu(self, pos) -> None:
        if not self.selectedItems():
            return
        menu = QMenu(self)

        copy_tsv = QAction(self._t("context.copy"), self)
        copy_tsv.setShortcut("Ctrl+C")
        copy_tsv.triggered.connect(lambda: self._copy_selection("tsv"))
        menu.addAction(copy_tsv)

        copy_csv = QAction(self._t("context.copy_csv"), self)
        copy_csv.triggered.connect(lambda: self._copy_selection("csv"))
        menu.addAction(copy_csv)

        copy_md = QAction(self._t("context.copy_markdown"), self)
        copy_md.triggered.connect(lambda: self._copy_selection("markdown"))
        menu.addAction(copy_md)

        menu.addSeparator()

        copy_with_headers = QAction(self._t("context.copy_with_headers"), self)
        copy_with_headers.triggered.connect(
            lambda: self._copy_selection("tsv", with_headers=True)
        )
        menu.addAction(copy_with_headers)

        menu.exec(self.viewport().mapToGlobal(pos))

    def _selection_grid(self) -> tuple[list[int], list[int], list[list[str]]]:
        """Return (rows, cols, grid) for the current rectangular selection.

        Cells outside the bounding box of the selection are filled with empty
        strings; selected ranges are normalized to a dense rectangle so the
        clipboard output is well-formed.
        """
        items = self.selectedItems()
        if not items:
            return [], [], []
        rows = sorted({i.row() for i in items})
        cols = sorted({i.column() for i in items})
        selected = {(i.row(), i.column()): i.text() for i in items}
        grid = [[selected.get((r, c), "") for c in cols] for r in rows]
        return rows, cols, grid

    def _copy_selection(self, fmt: str, with_headers: bool = False) -> None:
        rows, cols, grid = self._selection_grid()
        if not grid:
            return

        if fmt == "tsv":
            sep = "\t"
            text = "\n".join(sep.join(row) for row in grid)
            if with_headers:
                headers = [self.horizontalHeaderItem(c).text() for c in cols]
                text = sep.join(headers) + "\n" + text
        elif fmt == "csv":
            import csv
            import io
            buf = io.StringIO()
            writer = csv.writer(buf, lineterminator="\n")
            if with_headers:
                writer.writerow([self.horizontalHeaderItem(c).text() for c in cols])
            for row in grid:
                writer.writerow(row)
            text = buf.getvalue().rstrip("\n")
        elif fmt == "markdown":
            headers = [self.horizontalHeaderItem(c).text() for c in cols]
            lines = ["| " + " | ".join(headers) + " |"]
            lines.append("| " + " | ".join("---" for _ in headers) + " |")
            for row in grid:
                lines.append("| " + " | ".join(cell.replace("|", "\\|") for cell in row) + " |")
            text = "\n".join(lines)
        else:
            return

        QApplication.clipboard().setText(text)
        logger.info(f"Copied {len(rows)}x{len(cols)} cells as {fmt}")

    def keyPressEvent(self, event) -> None:
        from PyQt6.QtGui import QKeySequence
        if event.matches(QKeySequence.StandardKey.Copy):
            self._copy_selection("tsv")
            return
        super().keyPressEvent(event)

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
