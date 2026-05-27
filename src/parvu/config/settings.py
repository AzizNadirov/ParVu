"""
Settings management for ParVu.

Replaces schemas.py with a cleaner, testable design.
"""
from __future__ import annotations

import json
import shutil
from pathlib import Path
from typing import Literal

from pydantic import BaseModel, Field
from loguru import logger

from parvu.infrastructure.paths import get_legacy_app_dir, resolve_settings_path


class Settings(BaseModel):
    """Application settings model."""

    # Data settings
    default_data_var_name: str = "data"
    default_limit: int | str = 1000
    default_sql_font_size: int | str = 12
    default_result_font_size: int | str = 10
    default_sql_query: str = "SELECT * FROM $(default_data_var_name)"
    default_sql_font: str = "Courier New"
    sql_keywords: list[str] = Field(default_factory=list)
    result_pagination_rows_per_page: int | str = 100
    save_file_history: str = "True"
    max_rows: int | str = 100000

    # Theme
    current_theme: str = "ParVu Light"

    # Language
    current_language: str = "en"

    # Large dataset warning settings
    enable_large_dataset_warning: bool = True
    warning_criteria: Literal["rows", "cells", "filesize"] = "rows"

    # Exit warning
    warn_on_exit_with_transforms: bool = True
    warning_threshold_rows: int = 1_000_000
    warning_threshold_cells: int = 10_000_000
    warning_threshold_filesize_mb: int = 100

    # Crash reporting
    bug_report_email: str = "eziznadirov@gmail.com"
    enable_crash_reporting: bool = True

    # Colors (legacy, kept for backward compatibility)
    colour_browseButton: str = "#E8F5E9"
    colour_sqlEdit: str = "#FFF9C4"
    colour_executeButton: str = "#E1F5FE"
    colour_resultTable: str = "#FFFFFF"
    colour_tableInfoButton: str = "#F3E5F5"

    # Directories
    user_app_settings_dir: Path = Field(default_factory=get_legacy_app_dir)
    recents_file: Path = Field(default_factory=lambda: resolve_settings_path() / "recents.json")
    settings_file: Path = Field(default_factory=lambda: resolve_settings_path() / "settings.json")
    usr_recents_file: Path = Field(default_factory=lambda: get_legacy_app_dir() / "history" / "recents.json")
    usr_settings_file: Path = Field(default_factory=lambda: get_legacy_app_dir() / "settings" / "settings.json")
    default_settings_file: Path = Field(default_factory=lambda: resolve_settings_path() / "default_settings.json")
    static_dir: Path = Field(default_factory=lambda: Path(__file__).parent.parent / "resources" / "static")
    user_logs_dir: Path = Field(default_factory=lambda: get_legacy_app_dir() / "logs")

    def model_post_init(self, __context) -> None:
        """Post-initialization processing."""
        self.sql_keywords = list(set(k.upper().strip() for k in self.sql_keywords))

    def render_vars(self, query: str) -> str:
        """Render template variables inside a query string."""
        if not isinstance(query, str):
            return query
        query = query.replace("$(default_data_var_name)", str(self.default_data_var_name))
        query = query.replace("$(default_limit)", str(self.default_limit))
        query = query.replace("$(default_sql_font_size)", str(self.default_sql_font_size))
        query = query.replace("$(default_sql_query)", str(self.default_sql_query))
        query = query.replace("$(default_sql_font)", str(self.default_sql_font))
        return query


class SettingsManager:
    """Manages loading, saving, and resetting application settings."""

    def __init__(self, settings: Settings | None = None):
        self._settings = settings

    @property
    def settings(self) -> Settings:
        """Get current settings, loading if necessary."""
        if self._settings is None:
            self._settings = self.load()
        return self._settings

    @classmethod
    def load(cls) -> Settings:
        """Load settings from user directory, creating defaults if needed."""
        user_dir = get_legacy_app_dir()
        user_settings_file = user_dir / "settings" / "settings.json"

        if not user_dir.exists():
            cls.reset()

        try:
            with open(user_settings_file, "r") as f:
                data = json.load(f)
            settings = Settings.model_validate(data)
            logger.info("Settings loaded from user directory")
            return settings
        except Exception as e:
            logger.error(f"Failed to load settings: {e}")
            logger.critical("Resetting user settings")
            cls.reset()
            with open(user_settings_file, "r") as f:
                data = json.load(f)
            return Settings.model_validate(data)

    @classmethod
    def reset(cls) -> None:
        """Reset user settings to defaults."""
        user_dir = get_legacy_app_dir()
        bundled_settings = resolve_settings_path()
        bundled_history = Path(__file__).parent.parent / "resources" / "history"

        # Copy defaults
        shutil.copytree(bundled_settings, user_dir / "settings", dirs_exist_ok=True)
        shutil.copytree(bundled_history, user_dir / "history", dirs_exist_ok=True)
        logger.info("User settings reset to defaults")

    def save(self) -> None:
        """Save current settings to file."""
        if self._settings is None:
            return
        try:
            self._settings.usr_settings_file.parent.mkdir(parents=True, exist_ok=True)
            with open(self._settings.usr_settings_file, "w") as f:
                f.write(self._settings.model_dump_json(indent=2))
            logger.info("Settings saved")
        except Exception as e:
            logger.error(f"Failed to save settings: {e}")
            raise

    def update(self, **kwargs) -> None:
        """Update settings values and save."""
        for key, value in kwargs.items():
            if hasattr(self.settings, key):
                setattr(self.settings, key, value)
            else:
                logger.warning(f"Unknown setting: {key}")
        self.save()
