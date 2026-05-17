"""
Settings Dialog for ParVu.
"""
from __future__ import annotations

from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QTabWidget,
    QWidget, QLabel, QLineEdit, QPushButton, QComboBox,
    QSpinBox, QCheckBox, QGroupBox, QFormLayout,
    QMessageBox, QTextBrowser, QRadioButton, QButtonGroup,
)
from PyQt6.QtCore import pyqtSignal
from PyQt6.QtGui import QFont

from parvu.config.settings import Settings, SettingsManager
from parvu.infrastructure.themes.manager import ThemeManager
from parvu.infrastructure.i18n.base import I18n
from parvu.presentation.dialogs.language_selector import LanguageSelector


class SettingsDialog(QDialog):
    """Settings dialog with multiple tabs."""

    settings_changed = pyqtSignal()
    theme_changed = pyqtSignal(str)

    def __init__(
        self,
        settings: Settings,
        settings_manager: SettingsManager,
        theme_manager: ThemeManager,
        i18n: I18n,
        current_theme_name: str | None = None,
        parent=None,
    ):
        super().__init__(parent)
        self._settings = settings
        self._settings_manager = settings_manager
        self._theme_manager = theme_manager
        self._i18n = i18n
        self._current_theme_name = current_theme_name
        self._setup_ui()
        self._load_values()

    def _setup_ui(self) -> None:
        self.setWindowTitle("Settings")
        self.resize(700, 600)

        layout = QVBoxLayout(self)
        self._tabs = QTabWidget()

        self._tabs.addTab(self._create_general_tab(), "General")
        self._tabs.addTab(self._create_theme_tab(), "Theme")
        self._tabs.addTab(self._create_advanced_tab(), "Advanced")
        self._tabs.addTab(self._create_warnings_tab(), "Warnings")
        layout.addWidget(self._tabs)

        btn_layout = QHBoxLayout()
        save_btn = QPushButton("Save")
        save_btn.clicked.connect(self._save)
        btn_layout.addWidget(save_btn)

        cancel_btn = QPushButton("Cancel")
        cancel_btn.clicked.connect(self.reject)
        btn_layout.addWidget(cancel_btn)
        btn_layout.addStretch()
        layout.addLayout(btn_layout)

    def _create_general_tab(self) -> QWidget:
        w = QWidget()
        layout = QVBoxLayout(w)

        data_group = QGroupBox("Data Settings")
        data_layout = QFormLayout()
        self._table_var_edit = QLineEdit()
        data_layout.addRow("Table Variable Name:", self._table_var_edit)

        self._rows_spin = QSpinBox()
        self._rows_spin.setRange(10, 10000)
        data_layout.addRow("Rows Per Page:", self._rows_spin)

        self._max_rows_spin = QSpinBox()
        self._max_rows_spin.setRange(100, 100000)
        data_layout.addRow("Max Rows (LIMIT):", self._max_rows_spin)
        data_group.setLayout(data_layout)
        layout.addWidget(data_group)

        history_group = QGroupBox("File History")
        history_layout = QFormLayout()
        self._save_history_check = QCheckBox()
        history_layout.addRow("Save File History:", self._save_history_check)
        history_group.setLayout(history_layout)
        layout.addWidget(history_group)

        lang_group = QGroupBox("Language")
        lang_layout = QVBoxLayout()
        self._lang_selector = LanguageSelector(self._i18n)
        lang_layout.addWidget(self._lang_selector)
        lang_group.setLayout(lang_layout)
        layout.addWidget(lang_group)

        layout.addStretch()
        return w

    def _create_theme_tab(self) -> QWidget:
        w = QWidget()
        layout = QVBoxLayout(w)

        group = QGroupBox("Theme Selection")
        group_layout = QVBoxLayout()

        combo_layout = QFormLayout()
        self._theme_combo = QComboBox()
        self._theme_combo.currentTextChanged.connect(self._preview_theme)
        combo_layout.addRow("Active Theme:", self._theme_combo)
        group_layout.addLayout(combo_layout)

        preview_label = QLabel("Theme Preview:")
        preview_label.setFont(QFont("Arial", 9, QFont.Weight.Bold))
        group_layout.addWidget(preview_label)

        self._theme_preview = QTextBrowser()
        self._theme_preview.setMaximumHeight(300)
        group_layout.addWidget(self._theme_preview)

        btn_layout = QHBoxLayout()
        import_btn = QPushButton("Import...")
        import_btn.clicked.connect(self._import_theme)
        btn_layout.addWidget(import_btn)

        export_btn = QPushButton("Export...")
        export_btn.clicked.connect(self._export_theme)
        btn_layout.addWidget(export_btn)
        btn_layout.addStretch()
        group_layout.addLayout(btn_layout)

        group.setLayout(group_layout)
        layout.addWidget(group)
        layout.addStretch()
        return w

    def _create_advanced_tab(self) -> QWidget:
        w = QWidget()
        layout = QVBoxLayout(w)

        font_group = QGroupBox("Default Fonts")
        font_layout = QFormLayout()
        self._sql_font_edit = QLineEdit()
        font_layout.addRow("SQL Editor Font:", self._sql_font_edit)

        self._sql_font_size_spin = QSpinBox()
        self._sql_font_size_spin.setRange(8, 24)
        font_layout.addRow("SQL Font Size:", self._sql_font_size_spin)

        self._table_font_size_spin = QSpinBox()
        self._table_font_size_spin.setRange(7, 20)
        font_layout.addRow("Table Font Size:", self._table_font_size_spin)
        font_group.setLayout(font_layout)
        layout.addWidget(font_group)

        sql_group = QGroupBox("SQL Settings")
        sql_layout = QFormLayout()
        self._default_query_edit = QLineEdit()
        sql_layout.addRow("Default Query:", self._default_query_edit)

        self._default_limit_spin = QSpinBox()
        self._default_limit_spin.setRange(1, 10000)
        sql_layout.addRow("Default LIMIT:", self._default_limit_spin)
        sql_group.setLayout(sql_layout)
        layout.addWidget(sql_group)

        layout.addStretch()
        return w

    def _create_warnings_tab(self) -> QWidget:
        w = QWidget()
        layout = QVBoxLayout(w)

        enable_group = QGroupBox("Large Dataset Warning")
        enable_layout = QVBoxLayout()
        self._enable_warning_check = QCheckBox("Enable warning when loading unique values on large datasets")
        self._enable_warning_check.toggled.connect(self._on_warning_toggled)
        enable_layout.addWidget(self._enable_warning_check)
        enable_group.setLayout(enable_layout)
        layout.addWidget(enable_group)

        criteria_group = QGroupBox("Warning Criteria")
        criteria_layout = QVBoxLayout()
        self._criteria_group = QButtonGroup()

        self._rows_radio = QRadioButton("Row count")
        self._criteria_group.addButton(self._rows_radio, 0)
        criteria_layout.addWidget(self._rows_radio)

        rows_layout = QHBoxLayout()
        rows_layout.addSpacing(30)
        rows_layout.addWidget(QLabel("Threshold:"))
        self._rows_threshold = QSpinBox()
        self._rows_threshold.setRange(1000, 100_000_000)
        self._rows_threshold.setSingleStep(100_000)
        self._rows_threshold.setSuffix(" rows")
        rows_layout.addWidget(self._rows_threshold)
        rows_layout.addStretch()
        criteria_layout.addLayout(rows_layout)

        self._cells_radio = QRadioButton("Cell count (rows × columns)")
        self._criteria_group.addButton(self._cells_radio, 1)
        criteria_layout.addWidget(self._cells_radio)

        cells_layout = QHBoxLayout()
        cells_layout.addSpacing(30)
        cells_layout.addWidget(QLabel("Threshold:"))
        self._cells_threshold = QSpinBox()
        self._cells_threshold.setRange(10_000, 1_000_000_000)
        self._cells_threshold.setSingleStep(1_000_000)
        self._cells_threshold.setSuffix(" cells")
        cells_layout.addWidget(self._cells_threshold)
        cells_layout.addStretch()
        criteria_layout.addLayout(cells_layout)

        self._filesize_radio = QRadioButton("File size")
        self._criteria_group.addButton(self._filesize_radio, 2)
        criteria_layout.addWidget(self._filesize_radio)

        filesize_layout = QHBoxLayout()
        filesize_layout.addSpacing(30)
        filesize_layout.addWidget(QLabel("Threshold:"))
        self._filesize_threshold = QSpinBox()
        self._filesize_threshold.setRange(1, 100_000)
        self._filesize_threshold.setSingleStep(100)
        self._filesize_threshold.setSuffix(" MB")
        filesize_layout.addWidget(self._filesize_threshold)
        filesize_layout.addStretch()
        criteria_layout.addLayout(filesize_layout)

        criteria_group.setLayout(criteria_layout)
        layout.addWidget(criteria_group)
        layout.addStretch()
        return w

    def _load_values(self) -> None:
        s = self._settings
        self._table_var_edit.setText(s.default_data_var_name)
        self._rows_spin.setValue(int(s.result_pagination_rows_per_page))
        self._max_rows_spin.setValue(int(s.max_rows))
        self._save_history_check.setChecked(
            s.save_file_history in ("True", "true", "1", True, 1)
        )

        themes = self._theme_manager.list_themes()
        self._theme_combo.clear()
        self._theme_combo.addItems(themes)
        if self._current_theme_name:
            idx = self._theme_combo.findText(self._current_theme_name)
            if idx >= 0:
                self._theme_combo.setCurrentIndex(idx)

        self._sql_font_edit.setText(s.default_sql_font)
        self._sql_font_size_spin.setValue(int(s.default_sql_font_size))
        self._table_font_size_spin.setValue(int(s.default_result_font_size))
        self._default_query_edit.setText(s.default_sql_query)
        self._default_limit_spin.setValue(int(s.default_limit))

        self._enable_warning_check.setChecked(s.enable_large_dataset_warning)
        self._rows_threshold.setValue(s.warning_threshold_rows)
        self._cells_threshold.setValue(s.warning_threshold_cells)
        self._filesize_threshold.setValue(s.warning_threshold_filesize_mb)

        if s.warning_criteria == "rows":
            self._rows_radio.setChecked(True)
        elif s.warning_criteria == "cells":
            self._cells_radio.setChecked(True)
        elif s.warning_criteria == "filesize":
            self._filesize_radio.setChecked(True)

        self._on_warning_toggled(s.enable_large_dataset_warning)

    def _on_warning_toggled(self, checked: bool) -> None:
        self._rows_radio.setEnabled(checked)
        self._cells_radio.setEnabled(checked)
        self._filesize_radio.setEnabled(checked)
        self._rows_threshold.setEnabled(checked)
        self._cells_threshold.setEnabled(checked)
        self._filesize_threshold.setEnabled(checked)

    def _preview_theme(self, theme_name: str) -> None:
        theme = self._theme_manager.get_theme(theme_name)
        if not theme:
            self._theme_preview.setMarkdown("*Theme not found*")
            return
        preview = f"""# {theme.name}

**Author:** {theme.author}  
**Description:** {theme.description}

## Color Palette

- **Background:** `{theme.colors.background}`
- **Primary Accent:** `{theme.colors.accent_primary}`
- **SQL Keywords:** `{theme.colors.editor_keyword}`
- **Table Headers:** `{theme.colors.table_header_background}`

## Fonts

- **UI:** {theme.layout.default_font_family} ({theme.layout.default_font_size}pt)
- **Code:** {theme.layout.code_font_family} ({theme.layout.code_font_size}pt)
- **Table:** {theme.layout.table_font_family} ({theme.layout.table_font_size}pt)
"""
        self._theme_preview.setMarkdown(preview)

    def _import_theme(self) -> None:
        from PyQt6.QtWidgets import QFileDialog
        file_path, _ = QFileDialog.getOpenFileName(
            self, "Import Theme", "", "Theme Files (*.json);;All Files (*)"
        )
        if file_path:
            from pathlib import Path
            name = self._theme_manager.import_theme(Path(file_path))
            if name:
                QMessageBox.information(self, "Import Successful", f"Theme '{name}' imported!")
                self._load_values()
                idx = self._theme_combo.findText(name)
                if idx >= 0:
                    self._theme_combo.setCurrentIndex(idx)
            else:
                QMessageBox.critical(self, "Import Failed", "Check the file format.")

    def _export_theme(self) -> None:
        from PyQt6.QtWidgets import QFileDialog
        from pathlib import Path
        theme_name = self._theme_combo.currentText()
        if not theme_name:
            return
        default_name = theme_name.lower().replace(" ", "_") + ".json"
        file_path, _ = QFileDialog.getSaveFileName(
            self, "Export Theme", default_name, "Theme Files (*.json);;All Files (*)"
        )
        if file_path:
            success = self._theme_manager.export_theme(theme_name, Path(file_path))
            if success:
                QMessageBox.information(self, "Export Successful", f"Theme exported to:\n{file_path}")
            else:
                QMessageBox.critical(self, "Export Failed", "Failed to export theme.")

    def _save(self) -> None:
        table_var = self._table_var_edit.text().strip()
        if not table_var or table_var.upper() in self._settings.sql_keywords:
            QMessageBox.critical(self, "Invalid Setting", "Table variable name cannot be empty or a SQL keyword.")
            return

        self._settings.default_data_var_name = table_var
        self._settings.result_pagination_rows_per_page = str(self._rows_spin.value())
        self._settings.max_rows = str(self._max_rows_spin.value())
        self._settings.save_file_history = str(self._save_history_check.isChecked())

        self._settings.default_sql_font = self._sql_font_edit.text().strip()
        self._settings.default_sql_font_size = str(self._sql_font_size_spin.value())
        self._settings.default_result_font_size = str(self._table_font_size_spin.value())
        self._settings.default_sql_query = self._default_query_edit.text().strip()
        self._settings.default_limit = str(self._default_limit_spin.value())

        self._settings.enable_large_dataset_warning = self._enable_warning_check.isChecked()
        self._settings.warning_threshold_rows = self._rows_threshold.value()
        self._settings.warning_threshold_cells = self._cells_threshold.value()
        self._settings.warning_threshold_filesize_mb = self._filesize_threshold.value()

        if self._rows_radio.isChecked():
            self._settings.warning_criteria = "rows"
        elif self._cells_radio.isChecked():
            self._settings.warning_criteria = "cells"
        elif self._filesize_radio.isChecked():
            self._settings.warning_criteria = "filesize"

        selected_lang = self._lang_selector.get_selected_language()
        if selected_lang:
            self._settings.current_language = selected_lang

        self._settings_manager.save()

        selected_theme = self._theme_combo.currentText()
        if selected_theme != self._current_theme_name:
            self.theme_changed.emit(selected_theme)

        self.settings_changed.emit()
        self.accept()
