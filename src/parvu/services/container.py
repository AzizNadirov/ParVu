"""
Dependency Injection container for ParVu.

Wires all components together at application startup.
"""
from __future__ import annotations

import uuid
from datetime import datetime
from pathlib import Path
from typing import Any

from loguru import logger

from parvu.config.settings import SettingsManager, Settings
from parvu.config.recents import RecentsManager
from parvu.config.app_state import AppState
from parvu.infrastructure.i18n.base import I18n
from parvu.infrastructure.i18n.translator import create_translator, _Translator
from parvu.infrastructure.themes.manager import ThemeManager
from parvu.infrastructure.logging_config import setup_logging
from parvu.infrastructure.paths import get_log_dir, get_legacy_app_dir
from parvu.core.file_adapters import FileAdapterRegistry
from parvu.services.file_service import FileService
from parvu.services.query_service import QueryService


class ServiceContainer:
    """
    Central dependency injection container.

    Responsible for creating and wiring all application services.
    """

    def __init__(self):
        # Core infrastructure
        self.settings_manager = SettingsManager()
        self.settings = self.settings_manager.settings
        self.recents = RecentsManager(self.settings)
        self.theme_manager = ThemeManager()
        self.i18n = I18n()
        self.i18n.set_locale(self.settings.current_language)
        self.translator: _Translator = create_translator(self.settings.current_language)
        self.file_adapter_registry = FileAdapterRegistry()
        self.file_service = FileService(self.settings, self.recents, self.file_adapter_registry)
        self.query_service = QueryService()
        self.app_state = AppState()

        # Session
        self.session_id = self._generate_session_id()
        self.log_file = self._setup_logging()

        # Wire theme
        self.theme_manager.set_theme(self.settings.current_theme)

        logger.info(f"ServiceContainer initialized - Session: {self.session_id}")

    def _generate_session_id(self) -> str:
        """Generate a unique session identifier."""
        return (
            datetime.now().strftime("%Y%m%d_%H%M%S")
            + "_"
            + str(uuid.uuid4())[:8]
        )

    def _setup_logging(self) -> Path:
        """Setup session-based logging."""
        log_dir = get_log_dir()
        log_file = log_dir / f"parvu_session_{self.session_id}.log"
        setup_logging(log_file)
        return log_file

    def rebuild_translator(self, language_code: str) -> None:
        """Rebuild translator with new language."""
        self.translator = create_translator(language_code)

    def save_settings(self) -> None:
        """Persist current settings."""
        self.settings_manager.save()

    def create_query_engine(self, file_path: Path, page_size: int | None = None):
        """Factory method for creating query engines."""
        from parvu.core.query_engine import QueryEngine

        if page_size is None:
            page_size = int(self.settings.result_pagination_rows_per_page)

        return QueryEngine(
            file_path=file_path,
            page_size=page_size,
            table_name=self.settings.default_data_var_name,
            adapter_registry=self.file_adapter_registry,
        )
