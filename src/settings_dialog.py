"""
Settings Dialog for ParVu
"""
from PyQt6.QtWidgets import (QDialog, QVBoxLayout, QHBoxLayout, QTabWidget,
                             QWidget, QLabel, QLineEdit, QPushButton, QComboBox,
                             QSpinBox, QCheckBox, QGroupBox, QFormLayout,
                             QMessageBox, QTextBrowser, QRadioButton, QButtonGroup)
from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtGui import QFont
from loguru import logger

from schemas import settings
from themes import theme_manager
from language_selector import SimpleLanguageSelector
from i18n import get_i18n, t


class SettingsDialog(QDialog):
    """Settings dialog with theme selection"""

    settings_changed = pyqtSignal()
    theme_changed = pyqtSignal(str)  # theme_name

    def __init__(self, current_theme_name: str = None, parent=None):
        super().__init__(parent)
        self.current_theme_name = current_theme_name
        self.setup_ui()
        self.load_settings()

    def setup_ui(self):
        """Setup dialog UI"""
        self.setWindowTitle(t("dialog.settings"))
        self.resize(700, 600)

        layout = QVBoxLayout()

        # Tab widget
        self.tabs = QTabWidget()

        # General settings tab
        general_tab = self.create_general_tab()
        self.tabs.addTab(general_tab, t("settings.tab.general"))

        # Theme tab
        theme_tab = self.create_theme_tab()
        self.tabs.addTab(theme_tab, t("settings.tab.theme"))

        # Advanced tab
        advanced_tab = self.create_advanced_tab()
        self.tabs.addTab(advanced_tab, t("settings.tab.advanced"))

        # Warnings tab
        warnings_tab = self.create_warnings_tab()
        self.tabs.addTab(warnings_tab, t("settings.tab.warnings"))

        layout.addWidget(self.tabs)

        # Buttons
        button_layout = QHBoxLayout()

        self.save_btn = QPushButton(t("btn.save"))
        self.save_btn.clicked.connect(self.save_settings)
        button_layout.addWidget(self.save_btn)

        self.cancel_btn = QPushButton(t("btn.cancel"))
        self.cancel_btn.clicked.connect(self.reject)
        button_layout.addWidget(self.cancel_btn)

        button_layout.addStretch()
        layout.addLayout(button_layout)

        self.setLayout(layout)

    def create_general_tab(self) -> QWidget:
        """Create general settings tab"""
        widget = QWidget()
        layout = QVBoxLayout()

        # Data settings group
        data_group = QGroupBox(t("settings.group.data"))
        data_layout = QFormLayout()

        self.table_var_edit = QLineEdit()
        self.table_var_edit.setToolTip("Name used for table in SQL queries")
        data_layout.addRow("Table Variable Name:", self.table_var_edit)

        self.rows_per_page_spin = QSpinBox()
        self.rows_per_page_spin.setRange(10, 10000)
        self.rows_per_page_spin.setToolTip("Number of rows to display per page")
        data_layout.addRow("Rows Per Page:", self.rows_per_page_spin)

        self.max_rows_spin = QSpinBox()
        self.max_rows_spin.setRange(100, 100000)
        self.max_rows_spin.setToolTip("Maximum rows allowed in LIMIT clause")
        data_layout.addRow("Max Rows (LIMIT):", self.max_rows_spin)

        data_group.setLayout(data_layout)
        layout.addWidget(data_group)

        # File history group
        history_group = QGroupBox(t("settings.group.file_history"))
        history_layout = QFormLayout()

        self.save_history_check = QCheckBox()
        self.save_history_check.setToolTip("Save recently opened files")
        history_layout.addRow("Save File History:", self.save_history_check)

        history_group.setLayout(history_layout)
        layout.addWidget(history_group)

        # Language selection group
        language_group = QGroupBox(t("settings.group.language"))
        language_layout = QVBoxLayout()

        self.language_selector = SimpleLanguageSelector()
        self.language_selector.language_changed.connect(self.on_language_changed)
        language_layout.addWidget(self.language_selector)

        language_group.setLayout(language_layout)
        layout.addWidget(language_group)

        layout.addStretch()
        widget.setLayout(layout)
        return widget

    def create_theme_tab(self) -> QWidget:
        """Create theme settings tab"""
        widget = QWidget()
        layout = QVBoxLayout()

        # Theme selection group
        theme_group = QGroupBox(t("settings.group.theme_selection"))
        theme_layout = QVBoxLayout()

        # Theme combo box
        combo_layout = QFormLayout()
        self.theme_combo = QComboBox()
        self.theme_combo.currentTextChanged.connect(self.on_theme_preview)
        combo_layout.addRow("Active Theme:", self.theme_combo)
        theme_layout.addLayout(combo_layout)

        # Theme preview
        preview_label = QLabel(t("settings.group.theme_preview"))
        preview_label.setFont(QFont("Arial", 9, QFont.Weight.Bold))
        theme_layout.addWidget(preview_label)

        self.theme_preview = QTextBrowser()
        self.theme_preview.setMaximumHeight(300)
        theme_layout.addWidget(self.theme_preview)

        # Theme management buttons
        button_layout = QHBoxLayout()

        self.import_theme_btn = QPushButton(t("btn.import"))
        self.import_theme_btn.clicked.connect(self.import_theme)
        button_layout.addWidget(self.import_theme_btn)

        self.export_theme_btn = QPushButton(t("btn.export"))
        self.export_theme_btn.clicked.connect(self.export_theme)
        button_layout.addWidget(self.export_theme_btn)

        button_layout.addStretch()
        theme_layout.addLayout(button_layout)

        theme_group.setLayout(theme_layout)
        layout.addWidget(theme_group)

        layout.addStretch()
        widget.setLayout(layout)
        return widget

    def create_advanced_tab(self) -> QWidget:
        """Create advanced settings tab"""
        widget = QWidget()
        layout = QVBoxLayout()

        # Font settings group
        font_group = QGroupBox(t("settings.group.fonts"))
        font_layout = QFormLayout()

        self.sql_font_edit = QLineEdit()
        self.sql_font_edit.setToolTip("Font family for SQL editor")
        font_layout.addRow("SQL Editor Font:", self.sql_font_edit)

        self.sql_font_size_spin = QSpinBox()
        self.sql_font_size_spin.setRange(8, 24)
        self.sql_font_size_spin.setToolTip("Font size for SQL editor")
        font_layout.addRow("SQL Font Size:", self.sql_font_size_spin)

        self.table_font_size_spin = QSpinBox()
        self.table_font_size_spin.setRange(7, 20)
        self.table_font_size_spin.setToolTip("Font size for table data")
        font_layout.addRow("Table Font Size:", self.table_font_size_spin)

        font_group.setLayout(font_layout)
        layout.addWidget(font_group)

        # SQL settings group
        sql_group = QGroupBox(t("settings.group.sql"))
        sql_layout = QFormLayout()

        self.default_query_edit = QLineEdit()
        self.default_query_edit.setToolTip("Default SQL query template")
        sql_layout.addRow("Default Query:", self.default_query_edit)

        self.default_limit_spin = QSpinBox()
        self.default_limit_spin.setRange(1, 10000)
        self.default_limit_spin.setToolTip("Default LIMIT value")
        sql_layout.addRow("Default LIMIT:", self.default_limit_spin)

        sql_group.setLayout(sql_layout)
        layout.addWidget(sql_group)

        # Info label
        info_label = QLabel(
            "Note: Theme fonts and colors will override these defaults.\n"
            "Change theme in the Theme tab for comprehensive styling."
        )
        info_label.setWordWrap(True)
        info_label.setStyleSheet("color: #666; font-style: italic;")
        layout.addWidget(info_label)

        layout.addStretch()
        widget.setLayout(layout)
        return widget

    def create_warnings_tab(self) -> QWidget:
        """Create warnings settings tab"""
        widget = QWidget()
        layout = QVBoxLayout()

        # Enable/disable warning
        enable_group = QGroupBox(t("settings.group.large_dataset"))
        enable_layout = QVBoxLayout()

        self.enable_warning_check = QCheckBox("Enable warning when loading unique values on large datasets")
        self.enable_warning_check.setToolTip("Show warning dialog before calculating unique values on large datasets")
        self.enable_warning_check.toggled.connect(self.on_warning_enabled_toggled)
        enable_layout.addWidget(self.enable_warning_check)

        enable_group.setLayout(enable_layout)
        layout.addWidget(enable_group)

        # Warning criteria
        criteria_group = QGroupBox(t("settings.group.warning_criteria"))
        criteria_layout = QVBoxLayout()

        criteria_label = QLabel(t("settings.warning.trigger"))
        criteria_layout.addWidget(criteria_label)

        # Radio buttons for criteria
        self.criteria_button_group = QButtonGroup()

        self.rows_radio = QRadioButton(t("settings.warning.row_count"))
        self.criteria_button_group.addButton(self.rows_radio, 0)
        criteria_layout.addWidget(self.rows_radio)

        rows_threshold_layout = QHBoxLayout()
        rows_threshold_layout.addSpacing(30)
        rows_threshold_label = QLabel(t("settings.warning.threshold"))
        rows_threshold_layout.addWidget(rows_threshold_label)
        self.rows_threshold_spin = QSpinBox()
        self.rows_threshold_spin.setRange(1000, 100000000)
        self.rows_threshold_spin.setSingleStep(100000)
        self.rows_threshold_spin.setSuffix(" rows")
        self.rows_threshold_spin.setToolTip("Warn when row count exceeds this value")
        rows_threshold_layout.addWidget(self.rows_threshold_spin)
        rows_threshold_layout.addStretch()
        criteria_layout.addLayout(rows_threshold_layout)

        self.cells_radio = QRadioButton(t("settings.warning.cell_count"))
        self.criteria_button_group.addButton(self.cells_radio, 1)
        criteria_layout.addWidget(self.cells_radio)

        cells_threshold_layout = QHBoxLayout()
        cells_threshold_layout.addSpacing(30)
        cells_threshold_label = QLabel(t("settings.warning.threshold"))
        cells_threshold_layout.addWidget(cells_threshold_label)
        self.cells_threshold_spin = QSpinBox()
        self.cells_threshold_spin.setRange(10000, 1000000000)
        self.cells_threshold_spin.setSingleStep(1000000)
        self.cells_threshold_spin.setSuffix(" cells")
        self.cells_threshold_spin.setToolTip("Warn when total cell count (rows × columns) exceeds this value")
        cells_threshold_layout.addWidget(self.cells_threshold_spin)
        cells_threshold_layout.addStretch()
        criteria_layout.addLayout(cells_threshold_layout)

        self.filesize_radio = QRadioButton(t("settings.warning.file_size"))
        self.criteria_button_group.addButton(self.filesize_radio, 2)
        criteria_layout.addWidget(self.filesize_radio)

        filesize_threshold_layout = QHBoxLayout()
        filesize_threshold_layout.addSpacing(30)
        filesize_threshold_label = QLabel(t("settings.warning.threshold"))
        filesize_threshold_layout.addWidget(filesize_threshold_label)
        self.filesize_threshold_spin = QSpinBox()
        self.filesize_threshold_spin.setRange(1, 100000)
        self.filesize_threshold_spin.setSingleStep(100)
        self.filesize_threshold_spin.setSuffix(" MB")
        self.filesize_threshold_spin.setToolTip("Warn when file size exceeds this value")
        filesize_threshold_layout.addWidget(self.filesize_threshold_spin)
        filesize_threshold_layout.addStretch()
        criteria_layout.addLayout(filesize_threshold_layout)

        criteria_group.setLayout(criteria_layout)
        layout.addWidget(criteria_group)

        # Info label
        info_label = QLabel(
            "Note: Large dataset warnings help prevent long calculation times when "
            "requesting unique values for columns in very large files. The warning "
            "allows you to cancel before potentially slow operations begin."
        )
        info_label.setWordWrap(True)
        info_label.setStyleSheet("color: #666; font-style: italic;")
        layout.addWidget(info_label)

        layout.addStretch()
        widget.setLayout(layout)
        return widget

    def on_warning_enabled_toggled(self, checked: bool):
        """Enable/disable warning criteria controls"""
        self.rows_radio.setEnabled(checked)
        self.cells_radio.setEnabled(checked)
        self.filesize_radio.setEnabled(checked)
        self.rows_threshold_spin.setEnabled(checked)
        self.cells_threshold_spin.setEnabled(checked)
        self.filesize_threshold_spin.setEnabled(checked)

    def load_settings(self):
        """Load current settings into UI"""
        # General tab
        self.table_var_edit.setText(settings.default_data_var_name)
        self.rows_per_page_spin.setValue(int(settings.result_pagination_rows_per_page))
        self.max_rows_spin.setValue(int(settings.max_rows))
        self.save_history_check.setChecked(
            settings.save_file_history in ("True", "true", "1", True, 1)
        )

        # Theme tab
        themes = theme_manager.list_themes()
        self.theme_combo.clear()
        self.theme_combo.addItems(themes)

        # Select current theme
        if self.current_theme_name:
            index = self.theme_combo.findText(self.current_theme_name)
            if index >= 0:
                self.theme_combo.setCurrentIndex(index)

        # Advanced tab
        self.sql_font_edit.setText(settings.default_sql_font)
        self.sql_font_size_spin.setValue(int(settings.default_sql_font_size))
        self.table_font_size_spin.setValue(int(settings.default_result_font_size))
        self.default_query_edit.setText(settings.default_sql_query)
        self.default_limit_spin.setValue(int(settings.default_limit))

        # Warnings tab
        self.enable_warning_check.setChecked(settings.enable_large_dataset_warning)
        self.rows_threshold_spin.setValue(settings.warning_threshold_rows)
        self.cells_threshold_spin.setValue(settings.warning_threshold_cells)
        self.filesize_threshold_spin.setValue(settings.warning_threshold_filesize_mb)

        # Set radio button based on criteria
        if settings.warning_criteria == "rows":
            self.rows_radio.setChecked(True)
        elif settings.warning_criteria == "cells":
            self.cells_radio.setChecked(True)
        elif settings.warning_criteria == "filesize":
            self.filesize_radio.setChecked(True)
        else:
            self.rows_radio.setChecked(True)  # default

        # Update enabled state
        self.on_warning_enabled_toggled(settings.enable_large_dataset_warning)

    def on_theme_preview(self, theme_name: str):
        """Show theme preview"""
        theme = theme_manager.get_theme(theme_name)
        if not theme:
            self.theme_preview.setMarkdown("*Theme not found*")
            return

        # Build preview markdown
        preview = f"# {theme.name}\n\n"
        preview += f"**Author:** {theme.author}  \n"
        preview += f"**Description:** {theme.description}\n\n"

        preview += "## Color Palette\n\n"
        preview += f"- **Background:** `{theme.colors.background}`\n"
        preview += f"- **Primary Accent:** `{theme.colors.accent_primary}`\n"
        preview += f"- **SQL Keywords:** `{theme.colors.editor_keyword}`\n"
        preview += f"- **Table Headers:** `{theme.colors.table_header_background}`\n\n"

        preview += "## Fonts\n\n"
        preview += f"- **UI:** {theme.layout.default_font_family} ({theme.layout.default_font_size}pt)\n"
        preview += f"- **Code:** {theme.layout.code_font_family} ({theme.layout.code_font_size}pt)\n"
        preview += f"- **Table:** {theme.layout.table_font_family} ({theme.layout.table_font_size}pt)\n"

        self.theme_preview.setMarkdown(preview)

    def import_theme(self):
        """Import theme from file"""
        from PyQt6.QtWidgets import QFileDialog
        from pathlib import Path

        file_path, _ = QFileDialog.getOpenFileName(
            self,
            "Import Theme",
            "",
            "Theme Files (*.json);;All Files (*)"
        )

        if file_path:
            theme_name = theme_manager.import_theme(Path(file_path))
            if theme_name:
                # Refresh combo box
                self.load_settings()
                # Select the imported theme
                index = self.theme_combo.findText(theme_name)
                if index >= 0:
                    self.theme_combo.setCurrentIndex(index)
                QMessageBox.information(
                    self,
                    "Import Successful",
                    f"Theme '{theme_name}' imported successfully!"
                )
            else:
                QMessageBox.critical(
                    self,
                    "Import Failed",
                    "Failed to import theme. Check the file format."
                )

    def export_theme(self):
        """Export current theme"""
        from PyQt6.QtWidgets import QFileDialog
        from pathlib import Path

        theme_name = self.theme_combo.currentText()
        if not theme_name:
            return

        default_name = theme_name.lower().replace(' ', '_') + '.json'
        file_path, _ = QFileDialog.getSaveFileName(
            self,
            "Export Theme",
            default_name,
            "Theme Files (*.json);;All Files (*)"
        )

        if file_path:
            success = theme_manager.export_theme(theme_name, Path(file_path))
            if success:
                QMessageBox.information(
                    self,
                    "Export Successful",
                    f"Theme exported to:\n{file_path}"
                )
            else:
                QMessageBox.critical(
                    self,
                    "Export Failed",
                    "Failed to export theme."
                )

    def on_language_changed(self, locale_code: str):
        """Handle language selection change"""
        # Update i18n
        i18n = get_i18n()
        i18n.set_locale(locale_code)

        logger.info(f"Language changed to: {locale_code}")

        # Note: UI will update on next app restart
        # Could show a message here if desired

    def save_settings(self):
        """Save settings"""
        try:
            # Validate table var name
            table_var = self.table_var_edit.text().strip()
            if not table_var or table_var.upper() in settings.sql_keywords:
                QMessageBox.critical(
                    self,
                    "Invalid Setting",
                    "Table variable name cannot be empty or a SQL keyword."
                )
                return

            # Update settings
            settings.default_data_var_name = table_var
            settings.result_pagination_rows_per_page = str(self.rows_per_page_spin.value())
            settings.max_rows = str(self.max_rows_spin.value())
            settings.save_file_history = str(self.save_history_check.isChecked())

            settings.default_sql_font = self.sql_font_edit.text().strip()
            settings.default_sql_font_size = str(self.sql_font_size_spin.value())
            settings.default_result_font_size = str(self.table_font_size_spin.value())
            settings.default_sql_query = self.default_query_edit.text().strip()
            settings.default_limit = str(self.default_limit_spin.value())

            # Warning settings
            settings.enable_large_dataset_warning = self.enable_warning_check.isChecked()
            settings.warning_threshold_rows = self.rows_threshold_spin.value()
            settings.warning_threshold_cells = self.cells_threshold_spin.value()
            settings.warning_threshold_filesize_mb = self.filesize_threshold_spin.value()

            # Determine selected criteria
            if self.rows_radio.isChecked():
                settings.warning_criteria = "rows"
            elif self.cells_radio.isChecked():
                settings.warning_criteria = "cells"
            elif self.filesize_radio.isChecked():
                settings.warning_criteria = "filesize"

            # Save language preference
            selected_language = self.language_selector.get_selected_language()
            if selected_language:
                settings.current_language = selected_language

            # Save to file
            settings.save_settings()

            # Emit theme change if changed
            selected_theme = self.theme_combo.currentText()
            if selected_theme != self.current_theme_name:
                self.theme_changed.emit(selected_theme)

            self.settings_changed.emit()
            self.accept()

            logger.info("Settings saved successfully")

        except Exception as e:
            QMessageBox.critical(
                self,
                "Save Failed",
                f"Failed to save settings:\n{str(e)}"
            )
            logger.error(f"Failed to save settings: {e}")
