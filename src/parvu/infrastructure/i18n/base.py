"""
Internationalization base classes for ParVu.
"""
from __future__ import annotations

from typing import Dict
from loguru import logger

from parvu.infrastructure.i18n.locales import TRANSLATIONS_EN, TRANSLATIONS_RU, TRANSLATIONS_AZ


class Locale:
    """Represents a language locale with translations."""

    def __init__(
        self,
        code: str,
        name: str,
        native_name: str,
        flag: str,
        translations: Dict[str, str],
    ):
        self.code = code  # ISO 639-1 code (e.g., 'en', 'ru', 'az')
        self.name = name  # English name
        self.native_name = native_name  # Native name
        self.flag = flag  # Flag emoji
        self.translations = translations

    def translate(self, key: str, **kwargs) -> str:
        """Get translation for key with optional formatting."""
        text = self.translations.get(key, key)
        if kwargs:
            try:
                return text.format(**kwargs)
            except (KeyError, ValueError) as e:
                logger.warning(
                    f"Translation formatting error for key '{key}': {e}"
                )
                return text
        return text

    def __repr__(self) -> str:
        return f"Locale({self.code}, {self.native_name})"


class I18n:
    """Internationalization manager."""

    def __init__(self):
        self.locales: Dict[str, Locale] = {}
        self.current_locale: Locale | None = None
        self._initialize_locales()

    def _initialize_locales(self) -> None:
        """Initialize all supported locales."""
        self.locales["en"] = Locale(
            "en", "English", "English", "🇬🇧", TRANSLATIONS_EN
        )
        self.locales["ru"] = Locale(
            "ru", "Russian", "Русский", "🇷🇺", TRANSLATIONS_RU
        )
        self.locales["az"] = Locale(
            "az", "Azerbaijani", "Azərbaycan", "🇦🇿", TRANSLATIONS_AZ
        )
        self.current_locale = self.locales["en"]
        logger.info(f"Initialized {len(self.locales)} locales")

    def set_locale(self, code: str) -> bool:
        """Set current locale by code."""
        if code in self.locales:
            self.current_locale = self.locales[code]
            logger.info(
                f"Locale set to: {self.current_locale.native_name} ({code})"
            )
            return True
        logger.warning(f"Locale not found: {code}")
        return False

    def get_available_locales(self) -> list[Locale]:
        """Get list of available locales."""
        return list(self.locales.values())

    def translate(self, key: str, **kwargs) -> str:
        """Translate key."""
        if self.current_locale:
            return self.current_locale.translate(key, **kwargs)
        return key
