"""
Unit tests for theme system.
"""
from parvu.infrastructure.themes.manager import ThemeManager
from parvu.infrastructure.themes.models import Theme, ColorScheme


def test_theme_manager_builtin():
    manager = ThemeManager()
    themes = manager.list_themes()
    assert "ParVu Light" in themes
    assert "Excel" in themes
    assert "ParVu Black" in themes


def test_theme_manager_set_theme():
    manager = ThemeManager()
    assert manager.set_theme("ParVu Light")
    assert manager.current_theme is not None
    assert manager.current_theme.name == "ParVu Light"


def test_theme_manager_invalid_theme():
    manager = ThemeManager()
    assert not manager.set_theme("Nonexistent Theme")


def test_theme_to_json():
    theme = Theme(name="Test", colors=ColorScheme(background="#FFFFFF"))
    json_str = theme.to_json()
    assert "Test" in json_str
    assert "#FFFFFF" in json_str
