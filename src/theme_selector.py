"""
Theme Selector Dialog for ParVu
"""
from pathlib import Path
from PyQt6.QtWidgets import (QDialog, QVBoxLayout, QHBoxLayout, QPushButton,
                             QListWidget, QLabel, QTextBrowser, QFileDialog,
                             QMessageBox, QGroupBox)
from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtGui import QFont
from loguru import logger

from themes import theme_manager
from i18n import t


class ThemePreviewWidget(QGroupBox):
    """Widget showing theme preview"""

    def __init__(self, parent=None):
        super().__init__(t("theme.selector.preview"), parent)
        self.setup_ui()

    def setup_ui(self):
        """Setup preview UI"""
        layout = QVBoxLayout()

        self.info_browser = QTextBrowser()
        self.info_browser.setMaximumHeight(200)
        layout.addWidget(self.info_browser)

        self.setLayout(layout)

    def show_theme(self, theme_name: str):
        """Show theme preview"""
        theme = theme_manager.get_theme(theme_name)
        if not theme:
            self.info_browser.setMarkdown("*Theme not found*")
            return

        # Build preview markdown
        preview = f"# {theme.name}\n\n"
        preview += f"**Author:** {theme.author}\n\n"
        preview += f"**Description:** {theme.description}\n\n"
        preview += f"**Version:** {theme.version}\n\n"

        preview += "## Color Scheme\n\n"
        preview += f"- Background: `{theme.colors.background}`\n"
        preview += f"- Foreground: `{theme.colors.foreground}`\n"
        preview += f"- Accent Primary: `{theme.colors.accent_primary}`\n"
        preview += f"- SQL Keyword: `{theme.colors.editor_keyword}`\n"
        preview += f"- Table Selection: `{theme.colors.table_selection}`\n\n"

        preview += "## Layout\n\n"
        preview += f"- Default Font: {theme.layout.default_font_family} ({theme.layout.default_font_size}pt)\n"
        preview += f"- Code Font: {theme.layout.code_font_family} ({theme.layout.code_font_size}pt)\n"
        preview += f"- Show Grid: {theme.layout.show_grid}\n"
        preview += f"- Alternate Rows: {theme.layout.alternate_row_colors}\n"

        self.info_browser.setMarkdown(preview)


class ThemeSelectorDialog(QDialog):
    """Dialog for selecting and managing themes"""

    theme_selected = pyqtSignal(str)  # theme_name

    def __init__(self, current_theme_name: str = None, parent=None):
        super().__init__(parent)
        self.current_theme_name = current_theme_name
        self.selected_theme = current_theme_name
        self.setup_ui()
        self.load_themes()

    def setup_ui(self):
        """Setup dialog UI"""
        self.setWindowTitle(t("dialog.theme_selector"))
        self.resize(700, 600)

        main_layout = QVBoxLayout()

        # Info label
        info_label = QLabel(t("theme.selector.title"))
        info_label.setFont(QFont("Arial", 11, QFont.Weight.Bold))
        main_layout.addWidget(info_label)

        # Content layout (list + preview)
        content_layout = QHBoxLayout()

        # Left side - theme list and actions
        left_layout = QVBoxLayout()

        list_label = QLabel(t("theme.selector.available"))
        left_layout.addWidget(list_label)

        self.theme_list = QListWidget()
        self.theme_list.currentTextChanged.connect(self.on_theme_selected)
        self.theme_list.itemDoubleClicked.connect(self.on_apply_clicked)
        left_layout.addWidget(self.theme_list)

        # Theme management buttons
        mgmt_layout = QHBoxLayout()

        self.import_btn = QPushButton(t("btn.import"))
        self.import_btn.clicked.connect(self.import_theme)
        mgmt_layout.addWidget(self.import_btn)

        self.export_btn = QPushButton(t("btn.export"))
        self.export_btn.clicked.connect(self.export_theme)
        mgmt_layout.addWidget(self.export_btn)

        self.delete_btn = QPushButton(t("btn.delete"))
        self.delete_btn.clicked.connect(self.delete_theme)
        mgmt_layout.addWidget(self.delete_btn)

        left_layout.addLayout(mgmt_layout)

        content_layout.addLayout(left_layout, stretch=1)

        # Right side - preview
        self.preview_widget = ThemePreviewWidget()
        content_layout.addWidget(self.preview_widget, stretch=1)

        main_layout.addLayout(content_layout)

        # Bottom buttons
        button_layout = QHBoxLayout()

        self.apply_btn = QPushButton(t("btn.apply"))
        self.apply_btn.clicked.connect(self.on_apply_clicked)
        self.apply_btn.setDefault(True)
        button_layout.addWidget(self.apply_btn)

        self.cancel_btn = QPushButton(t("btn.cancel"))
        self.cancel_btn.clicked.connect(self.reject)
        button_layout.addWidget(self.cancel_btn)

        button_layout.addStretch()
        main_layout.addLayout(button_layout)

        self.setLayout(main_layout)

    def load_themes(self):
        """Load available themes into list"""
        self.theme_list.clear()

        themes = theme_manager.list_themes()
        for theme_name in themes:
            self.theme_list.addItem(theme_name)

        # Select current theme
        if self.current_theme_name:
            items = self.theme_list.findItems(self.current_theme_name, Qt.MatchFlag.MatchExactly)
            if items:
                self.theme_list.setCurrentItem(items[0])

    def on_theme_selected(self, theme_name: str):
        """Handle theme selection"""
        if theme_name:
            self.selected_theme = theme_name
            self.preview_widget.show_theme(theme_name)

            # Enable/disable delete button (can't delete built-in themes)
            is_builtin = theme_name in theme_manager.builtin_themes
            self.delete_btn.setEnabled(not is_builtin)

    def on_apply_clicked(self):
        """Apply selected theme"""
        if self.selected_theme:
            self.theme_selected.emit(self.selected_theme)
            self.accept()

    def import_theme(self):
        """Import theme from file"""
        file_path, _ = QFileDialog.getOpenFileName(
            self,
            "Import Theme",
            "",
            "Theme Files (*.json);;All Files (*)"
        )

        if file_path:
            theme_name = theme_manager.import_theme(Path(file_path))
            if theme_name:
                QMessageBox.information(
                    self,
                    "Import Successful",
                    f"Theme '{theme_name}' imported successfully!"
                )
                self.load_themes()
                # Select the imported theme
                items = self.theme_list.findItems(theme_name, Qt.MatchFlag.MatchExactly)
                if items:
                    self.theme_list.setCurrentItem(items[0])
            else:
                QMessageBox.critical(
                    self,
                    "Import Failed",
                    "Failed to import theme. Check the file format."
                )

    def export_theme(self):
        """Export selected theme"""
        if not self.selected_theme:
            QMessageBox.warning(self, "No Theme Selected", "Please select a theme to export.")
            return

        default_name = self.selected_theme.lower().replace(' ', '_') + '.json'
        file_path, _ = QFileDialog.getSaveFileName(
            self,
            "Export Theme",
            default_name,
            "Theme Files (*.json);;All Files (*)"
        )

        if file_path:
            success = theme_manager.export_theme(self.selected_theme, Path(file_path))
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

    def delete_theme(self):
        """Delete selected custom theme"""
        if not self.selected_theme:
            return

        # Confirm deletion
        reply = QMessageBox.question(
            self,
            "Delete Theme",
            f"Are you sure you want to delete the theme '{self.selected_theme}'?\n\n"
            f"This action cannot be undone.",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No
        )

        if reply == QMessageBox.StandardButton.Yes:
            success = theme_manager.delete_theme(self.selected_theme)
            if success:
                QMessageBox.information(
                    self,
                    "Theme Deleted",
                    f"Theme '{self.selected_theme}' deleted successfully."
                )
                self.load_themes()
            else:
                QMessageBox.warning(
                    self,
                    "Delete Failed",
                    "Cannot delete built-in themes."
                )
