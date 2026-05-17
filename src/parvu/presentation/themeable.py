"""
Themeable mixin for ParVu widgets.

Provides standardized theme application for all UI components.
"""
from __future__ import annotations

from typing import Protocol, runtime_checkable

from parvu.infrastructure.themes.models import Theme


@runtime_checkable
class IThemeable(Protocol):
    """Protocol for theme-aware widgets."""

    def apply_theme(self, theme: Theme) -> None: ...
    def update_theme(self, theme: Theme) -> None: ...


class ThemeableMixin:
    """Mixin that provides theme application helper methods."""

    def apply_theme(self, theme: Theme) -> None:
        """Apply theme to this widget. Override in subclasses."""
        pass

    def update_theme(self, theme: Theme) -> None:
        """Update theme (calls apply_theme by default)."""
        self.apply_theme(theme)
