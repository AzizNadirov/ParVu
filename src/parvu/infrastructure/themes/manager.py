"""
Theme manager for ParVu.

Manages built-in and custom themes.
"""
from __future__ import annotations

from pathlib import Path
from typing import Dict

from loguru import logger

from parvu.infrastructure.themes.models import Theme
from parvu.infrastructure.themes.stylesheet import generate_stylesheet
from parvu.infrastructure.themes.builtin import (
    light_theme,
    excel_theme,
    black_theme,
)
from parvu.infrastructure.paths import get_themes_dir


class ThemeManager:
    """Manages themes for the application."""

    def __init__(self, themes_dir: Path | None = None):
        self._themes_dir = themes_dir or get_themes_dir()
        self._themes_dir.mkdir(parents=True, exist_ok=True)

        self._current_theme: Theme | None = None
        self._builtin_themes: Dict[str, Theme] = {}

        self._create_builtin_themes()

        logger.info(
            f"ThemeManager initialized with {len(self._builtin_themes)} built-in themes"
        )

    def _create_builtin_themes(self) -> None:
        """Create built-in themes."""
        self._builtin_themes = {
            light_theme.name: light_theme,
            excel_theme.name: excel_theme,
            black_theme.name: black_theme,
        }

        # Save built-in themes to disk for reference
        for name, theme in self._builtin_themes.items():
            theme_file = self._themes_dir / f"{name.lower().replace(' ', '_')}.json"
            if not theme_file.exists():
                theme.save_to_file(theme_file)

    @property
    def current_theme(self) -> Theme | None:
        """Get the currently active theme."""
        return self._current_theme

    @property
    def builtin_themes(self) -> Dict[str, Theme]:
        """Get built-in themes dictionary."""
        return dict(self._builtin_themes)

    def get_theme(self, name: str) -> Theme | None:
        """Get theme by name (checks built-in first, then custom)."""
        if name in self._builtin_themes:
            return self._builtin_themes[name]

        theme_file = self._themes_dir / f"{name.lower().replace(' ', '_')}.json"
        if theme_file.exists():
            try:
                return Theme.load_from_file(theme_file)
            except Exception as e:
                logger.error(f"Failed to load theme {name}: {e}")
        return None

    def list_themes(self) -> list[str]:
        """List all available themes."""
        themes = list(self._builtin_themes.keys())
        for theme_file in self._themes_dir.glob("*.json"):
            try:
                theme = Theme.load_from_file(theme_file)
                if theme.name not in themes:
                    themes.append(theme.name)
            except Exception:
                pass
        return sorted(themes)

    def set_theme(self, name: str) -> bool:
        """Set current theme."""
        theme = self.get_theme(name)
        if theme:
            self._current_theme = theme
            logger.info(f"Theme set to: {name}")
            return True
        return False

    def export_theme(self, theme_name: str, export_path: Path) -> bool:
        """Export theme to file."""
        theme = self.get_theme(theme_name)
        if theme:
            try:
                theme.save_to_file(export_path)
                return True
            except Exception as e:
                logger.error(f"Failed to export theme: {e}")
        return False

    def import_theme(self, import_path: Path) -> str | None:
        """Import theme from file, returns theme name if successful."""
        try:
            theme = Theme.load_from_file(import_path)
            theme_file = (
                self._themes_dir / f"{theme.name.lower().replace(' ', '_')}.json"
            )
            theme.save_to_file(theme_file)
            logger.info(f"Imported theme: {theme.name}")
            return theme.name
        except Exception as e:
            logger.error(f"Failed to import theme: {e}")
            return None

    def delete_theme(self, name: str) -> bool:
        """Delete custom theme (cannot delete built-in themes)."""
        if name in self._builtin_themes:
            logger.warning(f"Cannot delete built-in theme: {name}")
            return False

        theme_file = self._themes_dir / f"{name.lower().replace(' ', '_')}.json"
        if theme_file.exists():
            try:
                theme_file.unlink()
                logger.info(f"Deleted theme: {name}")
                return True
            except Exception as e:
                logger.error(f"Failed to delete theme: {e}")
        return False

    def generate_stylesheet(self, theme: Theme | None = None) -> str:
        """Generate Qt stylesheet from theme."""
        t = theme or self._current_theme
        if t is None:
            return ""
        return generate_stylesheet(t)
