"""
pytest fixtures for ParVu.
"""
from __future__ import annotations

import pytest
from pathlib import Path

from parvu.config.settings import Settings
from parvu.config.recents import RecentsManager
from parvu.infrastructure.themes.manager import ThemeManager
from parvu.core.pagination import Paginator
from parvu.core.file_adapters import FileAdapterRegistry


@pytest.fixture
def temp_settings(tmp_path):
    """Create temporary settings for tests."""
    settings = Settings(
        user_app_settings_dir=tmp_path / ".ParVu",
        usr_settings_file=tmp_path / ".ParVu" / "settings" / "settings.json",
        usr_recents_file=tmp_path / ".ParVu" / "history" / "recents.json",
        static_dir=tmp_path / "static",
    )
    settings.user_app_settings_dir.mkdir(parents=True, exist_ok=True)
    return settings


@pytest.fixture
def recents_manager(temp_settings):
    """Create a recents manager with temporary settings."""
    return RecentsManager(temp_settings)


@pytest.fixture
def theme_manager():
    """Create a theme manager."""
    return ThemeManager()


@pytest.fixture
def paginator():
    """Create a paginator."""
    return Paginator(total_rows=1000, page_size=100)


@pytest.fixture
def adapter_registry():
    """Create a file adapter registry."""
    return FileAdapterRegistry()
