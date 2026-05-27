"""
Session service for ParVu.

Manages application lifecycle: initialization, shutdown, crash handling.
"""
from __future__ import annotations

import sys
import traceback
from pathlib import Path

from loguru import logger

from parvu.services.container import ServiceContainer


class SessionService:
    """Manages application session lifecycle."""

    def __init__(self, container: ServiceContainer):
        self._container = container
        self._original_excepthook = sys.excepthook

    def install_crash_handler(self) -> None:
        """Install global exception hook for crash reporting."""
        sys.excepthook = self._handle_exception

    def _handle_exception(
        self, exc_type, exc_value, exc_traceback
    ) -> None:
        """Handle uncaught exceptions."""
        logger.error("Uncaught exception", exc_info=(exc_type, exc_value, exc_traceback))

        if self._container.settings.enable_crash_reporting and self._container.log_file:
            try:
                from parvu.presentation.dialogs.crash_reporter import show_crash_report
                show_crash_report(exc_value, self._container.log_file, self._container.settings)
            except Exception as e:
                logger.error(f"Failed to show crash report: {e}")

        # Call original hook
        self._original_excepthook(exc_type, exc_value, exc_traceback)

    def shutdown(self) -> None:
        """Perform graceful shutdown."""
        logger.info("Shutting down session")
        sys.excepthook = self._original_excepthook
