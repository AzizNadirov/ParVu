"""
Language Selector Widget for ParVu
Allows users to select interface language
"""
from PyQt6.QtWidgets import (QWidget, QVBoxLayout, QHBoxLayout, QLabel,
                             QComboBox, QGroupBox)
from PyQt6.QtCore import pyqtSignal
from loguru import logger

from i18n import get_i18n, Locale


class LanguageSelector(QWidget):
    """Widget for selecting application language"""

    language_changed = pyqtSignal(str)  # Emits locale code when changed

    def __init__(self, parent=None):
        super().__init__(parent)
        self.i18n = get_i18n()
        self.setup_ui()

    def setup_ui(self):
        """Setup language selector UI"""
        layout = QVBoxLayout(self)

        # Group box
        group = QGroupBox("Interface Language")
        group_layout = QHBoxLayout()

        # Label
        self.label = QLabel("Language:")
        group_layout.addWidget(self.label)

        # Combo box with languages
        self.language_combo = QComboBox()
        self.populate_languages()
        self.language_combo.currentIndexChanged.connect(self.on_language_selected)
        group_layout.addWidget(self.language_combo, stretch=1)

        group.setLayout(group_layout)
        layout.addWidget(group)

    def populate_languages(self):
        """Populate combo box with available languages"""
        self.language_combo.clear()

        # Get all available locales
        locales = self.i18n.get_available_locales()

        # Sort by name for consistent display
        locales.sort(key=lambda loc: loc.name)

        # Add each locale
        for locale in locales:
            # Display: "🇬🇧 English (en)"
            display_text = f"{locale.flag} {locale.native_name} ({locale.code})"
            self.language_combo.addItem(display_text, locale.code)

        # Select current language
        self.select_current_language()

    def select_current_language(self):
        """Select the current language in combo box"""
        if not self.i18n.current_locale:
            return

        current_code = self.i18n.current_locale.code

        # Find and select the current language
        for i in range(self.language_combo.count()):
            if self.language_combo.itemData(i) == current_code:
                self.language_combo.setCurrentIndex(i)
                break

    def on_language_selected(self, index):
        """Handle language selection"""
        if index < 0:
            return

        locale_code = self.language_combo.itemData(index)

        # Only emit if language actually changed
        if self.i18n.current_locale and locale_code == self.i18n.current_locale.code:
            return

        logger.info(f"Language selected: {locale_code}")
        self.language_changed.emit(locale_code)

    def get_selected_language(self) -> str:
        """Get currently selected language code"""
        return self.language_combo.currentData()

    def update_ui_language(self):
        """Update widget text when language changes"""
        from i18n import t

        # Update labels
        self.label.setText(t("language.label"))

        # Re-populate combo to update text
        current_selection = self.get_selected_language()
        self.populate_languages()

        # Restore selection
        for i in range(self.language_combo.count()):
            if self.language_combo.itemData(i) == current_selection:
                self.language_combo.blockSignals(True)
                self.language_combo.setCurrentIndex(i)
                self.language_combo.blockSignals(False)
                break


class SimpleLanguageSelector(QWidget):
    """Simpler inline language selector without groupbox"""

    language_changed = pyqtSignal(str)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.i18n = get_i18n()
        self.setup_ui()

    def setup_ui(self):
        """Setup simple language selector"""
        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        # Label
        self.label = QLabel("Language:")
        layout.addWidget(self.label)

        # Combo box
        self.language_combo = QComboBox()
        self.populate_languages()
        self.language_combo.currentIndexChanged.connect(self.on_language_selected)
        layout.addWidget(self.language_combo, stretch=1)

    def populate_languages(self):
        """Populate combo box with available languages"""
        self.language_combo.clear()

        locales = self.i18n.get_available_locales()
        locales.sort(key=lambda loc: loc.name)

        for locale in locales:
            display_text = f"{locale.flag} {locale.native_name}"
            self.language_combo.addItem(display_text, locale.code)

        self.select_current_language()

    def select_current_language(self):
        """Select the current language in combo box"""
        if not self.i18n.current_locale:
            return

        current_code = self.i18n.current_locale.code

        for i in range(self.language_combo.count()):
            if self.language_combo.itemData(i) == current_code:
                self.language_combo.setCurrentIndex(i)
                break

    def on_language_selected(self, index):
        """Handle language selection"""
        if index < 0:
            return

        locale_code = self.language_combo.itemData(index)

        if self.i18n.current_locale and locale_code == self.i18n.current_locale.code:
            return

        logger.info(f"Language selected: {locale_code}")
        self.language_changed.emit(locale_code)

    def get_selected_language(self) -> str:
        """Get currently selected language code"""
        return self.language_combo.currentData()

    def update_ui_language(self):
        """Update widget text when language changes"""
        from i18n import t
        self.label.setText(t("language.name"))

        current_selection = self.get_selected_language()
        self.populate_languages()

        for i in range(self.language_combo.count()):
            if self.language_combo.itemData(i) == current_selection:
                self.language_combo.blockSignals(True)
                self.language_combo.setCurrentIndex(i)
                self.language_combo.blockSignals(False)
                break
