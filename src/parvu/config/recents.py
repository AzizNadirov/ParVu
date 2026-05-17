"""
Recent files management for ParVu.
"""
from __future__ import annotations

import json
from pathlib import Path

from pydantic import BaseModel
from loguru import logger

from parvu.config.settings import Settings


class RecentsData(BaseModel):
    """Raw recent files data."""
    recents: list[str] = []


class RecentsManager:
    """Manages recently opened files."""

    def __init__(self, settings: Settings):
        self._settings = settings
        self._data = self._load()

    def _load(self) -> RecentsData:
        """Load recents from file."""
        try:
            with open(self._settings.usr_recents_file, "r") as f:
                return RecentsData.model_validate_json(f.read())
        except Exception:
            return RecentsData()

    def save(self) -> None:
        """Save recents to file."""
        try:
            self._settings.usr_recents_file.parent.mkdir(parents=True, exist_ok=True)
            with open(self._settings.usr_recents_file, "w") as f:
                f.write(self._data.model_dump_json(indent=2))
        except Exception as e:
            logger.error(f"Failed to save recents: {e}")

    @property
    def recents(self) -> list[str]:
        """Get list of recent file paths."""
        return self._data.recents

    def add(self, path: str) -> None:
        """Add a file to recents."""
        # Move to front, remove duplicates
        if path in self._data.recents:
            self._data.recents.remove(path)
        self._data.recents.insert(0, path)
        self.save()

    def remove(self, path: str) -> None:
        """Remove a file from recents."""
        if path in self._data.recents:
            self._data.recents.remove(path)
            self.save()

    def clear(self) -> None:
        """Clear all recents."""
        self._data.recents = []
        self.save()

    def is_enabled(self) -> bool:
        """Check if file history saving is enabled."""
        val = self._settings.save_file_history
        return val in ("True", "true", "1", True, 1)
