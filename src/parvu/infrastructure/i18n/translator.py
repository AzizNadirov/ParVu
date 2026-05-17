"""
Translator service for ParVu.

Provides the t() translation function as a callable object,
making it injectable and testable.
"""
from __future__ import annotations

from typing import Any, Protocol, runtime_checkable

from parvu.infrastructure.i18n.base import I18n


@runtime_checkable
class Translator(Protocol):
    """Protocol for translation services."""

    def __call__(self, key: str, **kwargs: Any) -> str: ...
    def set_locale(self, code: str) -> bool: ...
    def get_available_locales(self) -> list[Any]: ...


class _Translator:
    """Concrete translator implementation."""

    def __init__(self, i18n: I18n):
        self._i18n = i18n

    def __call__(self, key: str, **kwargs: Any) -> str:
        return self._i18n.translate(key, **kwargs)

    def set_locale(self, code: str) -> bool:
        return self._i18n.set_locale(code)

    def get_available_locales(self) -> list[Any]:
        return self._i18n.get_available_locales()

    @property
    def current_locale_code(self) -> str:
        return self._i18n.current_locale.code if self._i18n.current_locale else "en"


def create_translator(language_code: str = "en") -> _Translator:
    """Factory function to create a translator instance."""
    i18n = I18n()
    i18n.set_locale(language_code)
    return _Translator(i18n)
