"""
File service for ParVu.

Handles file operations: open, validate, track recents.
"""
from __future__ import annotations

from pathlib import Path

from loguru import logger

from parvu.config.settings import Settings
from parvu.config.recents import RecentsManager
from parvu.core.exceptions import FileFormatError, FileNotFoundError
from parvu.core.file_adapters import FileAdapterRegistry


class FileService:
    """Manages file operations and recent files tracking."""

    def __init__(
        self,
        settings: Settings,
        recents: RecentsManager,
        adapter_registry: FileAdapterRegistry | None = None,
    ):
        self._settings = settings
        self._recents = recents
        self._adapter_registry = adapter_registry or FileAdapterRegistry()

    def validate_file(self, file_path: Path) -> None:
        """Validate that a file exists and is supported."""
        if not file_path.exists():
            raise FileNotFoundError(f"File does not exist: {file_path}")

        try:
            self._adapter_registry.get_adapter(file_path)
        except FileFormatError as e:
            raise FileFormatError(str(e))

    def add_to_recents(self, file_path: Path) -> None:
        """Add file to recent files if enabled."""
        if self._recents.is_enabled():
            self._recents.add(str(file_path))
            logger.info(f"Added to recents: {file_path}")

    def get_recent_files(self) -> list[str]:
        """Get list of recent file paths."""
        return self._recents.recents

    def remove_from_recents(self, file_path: str) -> None:
        """Remove a file from recents."""
        self._recents.remove(file_path)

    def clear_recents(self) -> None:
        """Clear recent files."""
        self._recents.clear()

    def get_file_dialog_filter(self) -> str:
        """Get QFileDialog filter for supported formats."""
        return self._adapter_registry.build_file_dialog_filter()

    def get_export_dialog_filter(self) -> str:
        """Get QFileDialog filter for export formats."""
        return self._adapter_registry.build_export_dialog_filter()
