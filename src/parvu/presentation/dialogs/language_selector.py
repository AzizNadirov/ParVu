"""
Language selector widgets for ParVu.
"""
from __future__ import annotations

from PyQt6.QtWidgets import QWidget, QVBoxLayout, QComboBox, QLabel
from PyQt6.QtCore import pyqtSignal

from parvu.infrastructure.i18n.base import I18n


class LanguageSelector(QWidget):
    """Language selection widget."""

    language_changed = pyqtSignal(str)

    def __init__(self, i18n: I18n, parent=None):
        super().__init__(parent)
        self._i18n = i18n
        self._setup_ui()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        self._combo = QComboBox()
        for locale in self._i18n.get_available_locales():
            self._combo.addItem(f"{locale.flag} {locale.native_name}", locale.code)

        if self._i18n.current_locale:
            idx = self._combo.findData(self._i18n.current_locale.code)
            if idx >= 0:
                self._combo.setCurrentIndex(idx)

        self._combo.currentIndexChanged.connect(self._on_changed)
        layout.addWidget(self._combo)

    def _on_changed(self, index: int) -> None:
        code = self._combo.itemData(index)
        if code:
            self.language_changed.emit(code)

    def get_selected_language(self) -> str:
        return self._combo.currentData() or "en"
