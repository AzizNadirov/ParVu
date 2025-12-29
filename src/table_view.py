"""
Custom Table View with Edit Support, Column Context Menu, and Unique Value Filters
"""
from PyQt6.QtWidgets import (QTableWidget, QTableWidgetItem, QMenu, QMessageBox,
                             QDialog, QVBoxLayout, QListWidget, QLineEdit, QPushButton,
                             QHBoxLayout, QLabel, QApplication, QListWidgetItem, QCheckBox)
from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtGui import QFont, QAction
import pandas as pd
from loguru import logger

from schemas import settings
from i18n import t


class UniqueValuesDialog(QDialog):
    """Dialog showing unique values for a column with multiselect checkboxes"""

    values_selected = pyqtSignal(str, list)  # column_name, list of selected values

    def __init__(self, column_name: str, unique_values: list, parent=None):
        super().__init__(parent)
        self.column_name = column_name
        self.unique_values = unique_values
        self.checkboxes = []  # Store checkbox references
        self.setup_ui()

    def setup_ui(self):
        """Setup dialog UI"""
        self.setWindowTitle(f"Unique Values - {self.column_name}")
        self.setModal(True)
        self.resize(400, 500)

        layout = QVBoxLayout()

        # Info label
        info_label = QLabel(f"Found {len(self.unique_values)} unique values (select multiple)")
        info_label.setFont(QFont("Arial", 10))
        layout.addWidget(info_label)

        # Search box
        self.search_box = QLineEdit()
        self.search_box.setPlaceholderText(t("label.search_placeholder"))
        self.search_box.textChanged.connect(self.filter_values)
        layout.addWidget(self.search_box)

        # Select All / Deselect All buttons
        select_layout = QHBoxLayout()

        select_all_btn = QPushButton(t("btn.select_all"))
        select_all_btn.clicked.connect(self.select_all)
        select_layout.addWidget(select_all_btn)

        deselect_all_btn = QPushButton(t("btn.deselect_all"))
        deselect_all_btn.clicked.connect(self.deselect_all)
        select_layout.addWidget(deselect_all_btn)

        layout.addLayout(select_layout)

        # List widget with checkboxes
        self.list_widget = QListWidget()
        # Sort unique values alphabetically (case-insensitive)
        sorted_values = sorted(self.unique_values, key=lambda x: str(x).lower())
        for value in sorted_values:
            item = QListWidgetItem()
            checkbox = QCheckBox(str(value))
            self.checkboxes.append(checkbox)
            self.list_widget.addItem(item)
            self.list_widget.setItemWidget(item, checkbox)
        layout.addWidget(self.list_widget)

        # Buttons
        button_layout = QHBoxLayout()

        filter_btn = QPushButton(t("btn.apply_filter"))
        filter_btn.setStyleSheet(
            "QPushButton {"
            "    background-color: #0288D1;"
            "    color: white;"
            "    font-weight: bold;"
            "    padding: 5px 15px;"
            "}"
            "QPushButton:hover {"
            "    background-color: #0277BD;"
            "}"
        )
        filter_btn.clicked.connect(self.on_filter_clicked)
        button_layout.addWidget(filter_btn)

        cancel_btn = QPushButton(t("btn.cancel"))
        cancel_btn.clicked.connect(self.reject)
        button_layout.addWidget(cancel_btn)

        layout.addLayout(button_layout)
        self.setLayout(layout)

    def select_all(self):
        """Select all visible checkboxes"""
        for i in range(self.list_widget.count()):
            item = self.list_widget.item(i)
            if not item.isHidden():
                checkbox = self.list_widget.itemWidget(item)
                if checkbox:
                    checkbox.setChecked(True)

    def deselect_all(self):
        """Deselect all checkboxes"""
        for checkbox in self.checkboxes:
            checkbox.setChecked(False)

    def filter_values(self, text: str):
        """Filter list based on search text"""
        for i in range(self.list_widget.count()):
            item = self.list_widget.item(i)
            checkbox = self.list_widget.itemWidget(item)
            if checkbox:
                item.setHidden(text.lower() not in checkbox.text().lower())

    def on_filter_clicked(self):
        """Handle filter button click - emit all selected values"""
        selected_values = []
        for checkbox in self.checkboxes:
            if checkbox.isChecked():
                selected_values.append(checkbox.text())

        if selected_values:
            self.values_selected.emit(self.column_name, selected_values)
            self.accept()
        else:
            QMessageBox.warning(self, "No Selection", "Please select at least one value to filter.")


class DataTableView(QTableWidget):
    """
    Enhanced table widget with:
    - Double-click to edit cells
    - Column header context menu (copy name, sort, unique values)
    - Copy cell value on single click
    """

    sort_requested = pyqtSignal(str, bool)  # column_name, ascending
    unique_values_requested = pyqtSignal(str)  # column_name
    filter_requested = pyqtSignal(str, list)  # column_name, list of values

    def __init__(self, parent=None, theme=None):
        super().__init__(parent)
        self.current_data = pd.DataFrame()
        self.theme = theme

        # Set font from theme or settings
        if theme:
            font = QFont(theme.layout.table_font_family, theme.layout.table_font_size)
        else:
            font = QFont("Courier", int(settings.default_result_font_size))
        self.setFont(font)

        # Apply theme settings
        if theme:
            self.setShowGrid(theme.layout.show_grid)
            self.setAlternatingRowColors(theme.layout.alternate_row_colors)

        # Enable editing on double-click
        self.setEditTriggers(QTableWidget.EditTrigger.DoubleClicked)

        # Enable custom context menu
        self.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.customContextMenuRequested.connect(self.show_context_menu)

        # Make headers clickable
        self.horizontalHeader().sectionClicked.connect(self.on_header_clicked)

    def load_data(self, df: pd.DataFrame):
        """
        Load DataFrame into table

        Args:
            df: Pandas DataFrame to display
        """
        self.current_data = df

        # Set dimensions
        self.setRowCount(len(df))
        self.setColumnCount(len(df.columns))
        self.setHorizontalHeaderLabels(df.columns.tolist())

        # Fill table
        for i in range(len(df)):
            for j, col in enumerate(df.columns):
                value = df.iloc[i, j]
                item = QTableWidgetItem(str(value))

                # Make cells editable but keep original value in data
                item.setData(Qt.ItemDataRole.UserRole, value)

                self.setItem(i, j, item)

        # Resize columns
        self.resizeColumnsToContents()

    def show_context_menu(self, pos):
        """Show context menu for column operations"""
        # Get column from position
        column = self.columnAt(pos.x())
        if column < 0:
            return

        column_name = self.horizontalHeaderItem(column).text()

        # Create context menu
        menu = QMenu(self)

        # Copy column name
        copy_name_action = QAction(t("context.copy_column"), self)
        copy_name_action.triggered.connect(lambda: self.copy_column_name(column_name))
        menu.addAction(copy_name_action)

        menu.addSeparator()

        # Sort ascending
        sort_asc_action = QAction(t("context.sort_asc"), self)
        sort_asc_action.triggered.connect(lambda: self.sort_requested.emit(column_name, True))
        menu.addAction(sort_asc_action)

        # Sort descending
        sort_desc_action = QAction(t("context.sort_desc"), self)
        sort_desc_action.triggered.connect(lambda: self.sort_requested.emit(column_name, False))
        menu.addAction(sort_desc_action)

        menu.addSeparator()

        # Copy values as tuple
        copy_tuple_action = QAction(t("context.copy_values"), self)
        copy_tuple_action.triggered.connect(lambda: self.copy_column_as_tuple(column))
        menu.addAction(copy_tuple_action)

        # Unique values dropdown
        unique_action = QAction(t("context.unique_values"), self)
        unique_action.triggered.connect(lambda: self.unique_values_requested.emit(column_name))
        menu.addAction(unique_action)

        # Show menu
        menu.exec(self.mapToGlobal(pos))

    def on_header_clicked(self, logical_index: int):
        """Handle header click - quick sort"""
        column_name = self.horizontalHeaderItem(logical_index).text()
        # Default to ascending on header click
        self.sort_requested.emit(column_name, True)

    def copy_column_name(self, column_name: str):
        """Copy column name to clipboard"""
        clipboard = QApplication.clipboard()
        clipboard.setText(column_name)
        logger.info(f"Copied column name: {column_name}")

    def copy_column_as_tuple(self, column: int):
        """Copy column values as Python tuple"""
        if self.current_data.empty:
            return

        column_name = self.horizontalHeaderItem(column).text()
        values = self.current_data[column_name].tolist()

        # Format as tuple
        tuple_str = "(" + ", ".join(repr(v) for v in values) + ")"

        clipboard = QApplication.clipboard()
        clipboard.setText(tuple_str)
        logger.info(f"Copied {len(values)} values as tuple")

    def get_edited_data(self) -> pd.DataFrame:
        """
        Get DataFrame with any user edits applied

        Returns:
            DataFrame with current table values
        """
        if self.current_data.empty:
            return pd.DataFrame()

        # Create copy of original data
        edited_df = self.current_data.copy()

        # Update with edited values
        for i in range(self.rowCount()):
            for j in range(self.columnCount()):
                item = self.item(i, j)
                if item:
                    col_name = self.horizontalHeaderItem(j).text()
                    edited_df.iloc[i, j] = item.text()

        return edited_df

    def update_theme(self, theme):
        """Update table theme"""
        self.theme = theme
        # Update font
        font = QFont(theme.layout.table_font_family, theme.layout.table_font_size)
        self.setFont(font)
        # Update settings
        self.setShowGrid(theme.layout.show_grid)
        self.setAlternatingRowColors(theme.layout.alternate_row_colors)
