"""
Theme Selector Dialog for ParVu.
"""
from __future__ import annotations

from pathlib import Path

from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QPushButton,
    QListWidget, QLabel, QTextBrowser, QFileDialog, QMessageBox,
    QGroupBox,
)
from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtGui import QFont

from parvu.infrastructure.themes.manager import ThemeManager


class ThemePreviewWidget(QGroupBox):
    """Widget showing theme preview."""

    def __init__(self, parent=None):
        super().__init__("Preview", parent)
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        self._browser = QTextBrowser()
        self._browser.setMaximumHeight(200)
        layout.addWidget(self._browser)

    def show_theme(self, theme) -> None:
        if not theme:
            self._browser.setMarkdown("*Theme not found*")
            return

        preview = f"""# {theme.name}

**Author:** {theme.author}

**Description:** {theme.description}

**Version:** {theme.version}

## Color Scheme

- Background: `{theme.colors.background}`
- Foreground: `{theme.colors.foreground}`
- Accent Primary: `{theme.colors.accent_primary}`
- SQL Keyword: `{theme.colors.editor_keyword}`
- Table Selection: `{theme.colors.table_selection}`

## Layout

- Default Font: {theme.layout.default_font_family} ({theme.layout.default_font_size}pt)
- Code Font: {theme.layout.code_font_family} ({theme.layout.code_font_size}pt)
- Show Grid: {theme.layout.show_grid}
- Alternate Rows: {theme.layout.alternate_row_colors}
"""
        self._browser.setMarkdown(preview)


class ThemeSelectorDialog(QDialog):
    """Dialog for selecting and managing themes."""

    theme_selected = pyqtSignal(str)

    def __init__(self, theme_manager: ThemeManager, current_theme_name: str | None = None, parent=None):
        super().__init__(parent)
        self._theme_manager = theme_manager
        self._current_theme_name = current_theme_name
        self._selected_theme = current_theme_name
        self._setup_ui()
        self._load_themes()

    def _setup_ui(self) -> None:
        self.setWindowTitle("Theme Selector")
        self.resize(700, 600)

        main_layout = QVBoxLayout(self)

        info = QLabel("Select a theme for ParVu")
        info.setFont(QFont("Arial", 11, QFont.Weight.Bold))
        main_layout.addWidget(info)

        content = QHBoxLayout()
        left = QVBoxLayout()

        left.addWidget(QLabel("Available Themes:"))
        self._theme_list = QListWidget()
        self._theme_list.currentTextChanged.connect(self._on_theme_selected)
        self._theme_list.itemDoubleClicked.connect(self._on_apply)
        left.addWidget(self._theme_list)

        mgmt = QHBoxLayout()
        import_btn = QPushButton("Import...")
        import_btn.clicked.connect(self._import_theme)
        mgmt.addWidget(import_btn)

        export_btn = QPushButton("Export...")
        export_btn.clicked.connect(self._export_theme)
        mgmt.addWidget(export_btn)

        delete_btn = QPushButton("Delete")
        delete_btn.clicked.connect(self._delete_theme)
        mgmt.addWidget(delete_btn)
        left.addLayout(mgmt)

        content.addLayout(left, stretch=1)

        self._preview = ThemePreviewWidget()
        content.addWidget(self._preview, stretch=1)
        main_layout.addLayout(content)

        btn_layout = QHBoxLayout()
        apply_btn = QPushButton("Apply")
        apply_btn.clicked.connect(self._on_apply)
        apply_btn.setDefault(True)
        btn_layout.addWidget(apply_btn)

        cancel_btn = QPushButton("Cancel")
        cancel_btn.clicked.connect(self.reject)
        btn_layout.addWidget(cancel_btn)
        btn_layout.addStretch()
        main_layout.addLayout(btn_layout)

    def _load_themes(self) -> None:
        self._theme_list.clear()
        for name in self._theme_manager.list_themes():
            self._theme_list.addItem(name)

        if self._current_theme_name:
            items = self._theme_list.findItems(
                self._current_theme_name, Qt.MatchFlag.MatchExactly
            )
            if items:
                self._theme_list.setCurrentItem(items[0])

    def _on_theme_selected(self, theme_name: str) -> None:
        if theme_name:
            self._selected_theme = theme_name
            theme = self._theme_manager.get_theme(theme_name)
            self._preview.show_theme(theme)
            is_builtin = theme_name in self._theme_manager.builtin_themes
            # Disable delete for built-in themes - would need to access delete_btn

    def _on_apply(self) -> None:
        if self._selected_theme:
            self.theme_selected.emit(self._selected_theme)
            self.accept()

    def _import_theme(self) -> None:
        file_path, _ = QFileDialog.getOpenFileName(
            self, "Import Theme", "", "Theme Files (*.json);;All Files (*)"
        )
        if file_path:
            name = self._theme_manager.import_theme(Path(file_path))
            if name:
                QMessageBox.information(self, "Import Successful", f"Theme '{name}' imported!")
                self._load_themes()
                items = self._theme_list.findItems(name, Qt.MatchFlag.MatchExactly)
                if items:
                    self._theme_list.setCurrentItem(items[0])
            else:
                QMessageBox.critical(self, "Import Failed", "Check the file format.")

    def _export_theme(self) -> None:
        if not self._selected_theme:
            QMessageBox.warning(self, "No Selection", "Select a theme to export.")
            return
        default_name = self._selected_theme.lower().replace(" ", "_") + ".json"
        file_path, _ = QFileDialog.getSaveFileName(
            self, "Export Theme", default_name, "Theme Files (*.json);;All Files (*)"
        )
        if file_path:
            success = self._theme_manager.export_theme(self._selected_theme, Path(file_path))
            if success:
                QMessageBox.information(self, "Export Successful", f"Theme exported to:\n{file_path}")
            else:
                QMessageBox.critical(self, "Export Failed", "Failed to export theme.")

    def _delete_theme(self) -> None:
        if not self._selected_theme:
            return
        reply = QMessageBox.question(
            self, "Delete Theme",
            f"Delete theme '{self._selected_theme}'?\n\nThis cannot be undone.",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
        )
        if reply == QMessageBox.StandardButton.Yes:
            success = self._theme_manager.delete_theme(self._selected_theme)
            if success:
                QMessageBox.information(self, "Deleted", f"Theme '{self._selected_theme}' deleted.")
                self._load_themes()
            else:
                QMessageBox.warning(self, "Delete Failed", "Cannot delete built-in themes.")
