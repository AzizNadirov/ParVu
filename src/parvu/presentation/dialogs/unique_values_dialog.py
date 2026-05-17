"""
Unique Values Dialog for ParVu.
"""
from __future__ import annotations

from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QPushButton,
    QListWidget, QLineEdit, QLabel, QListWidgetItem, QCheckBox,
    QMessageBox,
)
from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtGui import QFont


class UniqueValuesDialog(QDialog):
    """Dialog showing unique values for a column with multiselect."""

    values_selected = pyqtSignal(str, list)  # column_name, selected_values

    def __init__(self, column_name: str, unique_values: list, parent=None):
        super().__init__(parent)
        self._column_name = column_name
        self._unique_values = unique_values
        self._checkboxes: list[QCheckBox] = []
        self._setup_ui()

    def _setup_ui(self) -> None:
        self.setWindowTitle(f"Unique Values - {self._column_name}")
        self.setModal(True)
        self.resize(400, 500)

        layout = QVBoxLayout(self)

        info = QLabel(f"Found {len(self._unique_values)} unique values (select multiple)")
        info.setFont(QFont("Arial", 10))
        layout.addWidget(info)

        self._search_box = QLineEdit()
        self._search_box.setPlaceholderText("Search values...")
        self._search_box.textChanged.connect(self._filter_values)
        layout.addWidget(self._search_box)

        select_layout = QHBoxLayout()
        select_all = QPushButton("Select All")
        select_all.clicked.connect(self._select_all)
        select_layout.addWidget(select_all)

        deselect_all = QPushButton("Deselect All")
        deselect_all.clicked.connect(self._deselect_all)
        select_layout.addWidget(deselect_all)
        layout.addLayout(select_layout)

        self._list_widget = QListWidget()
        sorted_values = sorted(self._unique_values, key=lambda x: str(x).lower())
        for value in sorted_values:
            item = QListWidgetItem()
            checkbox = QCheckBox(str(value))
            self._checkboxes.append(checkbox)
            self._list_widget.addItem(item)
            self._list_widget.setItemWidget(item, checkbox)
        layout.addWidget(self._list_widget)

        btn_layout = QHBoxLayout()
        filter_btn = QPushButton("Apply Filter")
        filter_btn.clicked.connect(self._on_filter)
        btn_layout.addWidget(filter_btn)

        cancel_btn = QPushButton("Cancel")
        cancel_btn.clicked.connect(self.reject)
        btn_layout.addWidget(cancel_btn)
        layout.addLayout(btn_layout)

    def _select_all(self) -> None:
        for i in range(self._list_widget.count()):
            item = self._list_widget.item(i)
            if not item.isHidden():
                checkbox = self._list_widget.itemWidget(item)
                if checkbox:
                    checkbox.setChecked(True)

    def _deselect_all(self) -> None:
        for checkbox in self._checkboxes:
            checkbox.setChecked(False)

    def _filter_values(self, text: str) -> None:
        text = text.lower()
        for i in range(self._list_widget.count()):
            item = self._list_widget.item(i)
            checkbox = self._list_widget.itemWidget(item)
            if checkbox:
                item.setHidden(text not in checkbox.text().lower())

    def _on_filter(self) -> None:
        selected = []
        for checkbox in self._checkboxes:
            if checkbox.isChecked():
                selected.append(checkbox.text())

        if selected:
            self.values_selected.emit(self._column_name, selected)
            self.accept()
        else:
            QMessageBox.warning(self, "No Selection", "Please select at least one value.")
