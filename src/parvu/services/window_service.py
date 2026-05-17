"""
Window management service for ParVu.
"""
from __future__ import annotations

from pathlib import Path
from typing import Any

from loguru import logger

from parvu.services.container import ServiceContainer


class WindowService:
    """Manages multiple application windows."""

    def __init__(self, container: ServiceContainer):
        self._container = container
        self._windows: list[Any] = []

    @property
    def window_count(self) -> int:
        return len(self._windows)

    def create_window(self, file_path: Path | None = None) -> Any:
        """Create and show a new main window."""
        from parvu.presentation.main_window import MainWindow

        window = MainWindow(container=self._container, file_path=file_path)
        window.show()
        self._windows.append(window)
        logger.info(f"Created new window. Total windows: {len(self._windows)}")
        return window

    def remove_window(self, window: Any) -> None:
        """Remove window from tracking."""
        if window in self._windows:
            self._windows.remove(window)
            logger.info(f"Window closed. Remaining: {len(self._windows)}")

    def close_all(self) -> None:
        """Close all tracked windows."""
        for window in list(self._windows):
            window.close()
        self._windows.clear()
