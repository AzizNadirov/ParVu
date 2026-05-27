"""
Core interfaces and protocols for ParVu.

Defines contracts for all major components to enable:
- Dependency injection
- Testability via mocks
- Swappable implementations
"""
from __future__ import annotations

from abc import ABC, abstractmethod
from pathlib import Path
from typing import Protocol, runtime_checkable, Any

import pandas as pd


# ---------------------------------------------------------------------------
# Domain protocols
# ---------------------------------------------------------------------------

@runtime_checkable
class IQueryEngine(Protocol):
    """Protocol for SQL query engines."""

    @property
    def total_rows(self) -> int: ...

    @property
    def total_pages(self) -> int: ...

    @property
    def page_size(self) -> int: ...

    @property
    def file_path(self) -> Path: ...

    def execute_query(self, query: str) -> tuple[bool, str]:
        """Execute a SQL query. Returns (success, error_message)."""
        ...

    def get_page(self, page_num: int) -> pd.DataFrame:
        """Get a specific page of results (1-indexed)."""
        ...

    def get_columns(self) -> list[str]:
        """Get column names from current query result."""
        ...

    def get_column_types(self) -> list[tuple[str, str]]:
        """Get column names and types."""
        ...

    def sort_by_column(self, column: str, ascending: bool = True) -> tuple[bool, str]:
        """Sort current results by column. Returns (success, error_message)."""
        ...

    def get_unique_values(self, column: str) -> list[Any]:
        """Get unique values for a column."""
        ...

    def get_column_stats(self, column: str) -> dict[str, Any]:
        """Compute summary statistics for a column over the current query.

        Returns a dict with at least: ``row_count``, ``non_null``, ``null``,
        ``distinct``, ``min``, ``max``, ``mean``, ``std``. Numeric stats are
        ``None`` when the column is non-numeric. ``type`` carries DuckDB's
        column type string.
        """
        ...

    def find_next_match(
        self,
        query: str,
        column: str | None = None,
        after_row: int | None = None,
        after_col_index: int | None = None,
        direction: str = "next",
        case_sensitive: bool = False,
        whole_cell: bool = False,
        use_regex: bool = False,
    ) -> tuple[int, int, str, str] | None:
        """Find one match relative to an anchor (row, col_index).

        ``direction`` is ``"next"`` (strictly after the anchor, ascending order)
        or ``"prev"`` (strictly before, descending). With no anchor the search
        returns the first/last match in the table.

        Returns ``(absolute_row, column_index, column_name, value)`` or ``None``
        if no further match exists.
        """
        ...

    def export_results(self, output_path: Path) -> bool:
        """Export current query results to file."""
        ...

    @property
    def can_undo(self) -> bool:
        """Return True if there is at least one step to undo."""
        ...

    def undo(self) -> tuple[bool, str]:
        """Revert the last transformation. Returns (success, error_message)."""
        ...

    def reset_query(self) -> None:
        """Reset to original file reader query."""
        ...

    def get_table_info(self) -> dict[str, Any]:
        """Get table metadata."""
        ...

    def close(self) -> None:
        """Close database connection and release resources."""
        ...


@runtime_checkable
class IFileAdapter(Protocol):
    """Protocol for file format adapters."""

    @property
    def supported_extensions(self) -> set[str]: ...

    def can_handle(self, file_path: Path) -> bool:
        """Check if this adapter can handle the given file."""
        ...

    def build_reader_query(self, file_path: Path) -> str:
        """Build a DuckDB reader query for the file."""
        ...


@runtime_checkable
class IPaginator(Protocol):
    """Protocol for pagination logic."""

    @property
    def total_rows(self) -> int: ...

    @property
    def total_pages(self) -> int: ...

    @property
    def page_size(self) -> int: ...

    def page_offset(self, page_num: int) -> int:
        """Calculate OFFSET for a given page number (1-indexed)."""
        ...

    def clamp_page(self, page_num: int) -> int:
        """Clamp page number to valid range."""
        ...


# ---------------------------------------------------------------------------
# Infrastructure protocols
# ---------------------------------------------------------------------------

@runtime_checkable
class ISettings(Protocol):
    """Protocol for application settings."""

    default_data_var_name: str
    default_limit: int | str
    default_sql_font_size: int | str
    default_result_font_size: int | str
    default_sql_query: str
    default_sql_font: str
    sql_keywords: list[str]
    result_pagination_rows_per_page: int | str
    save_file_history: str
    max_rows: int | str
    current_theme: str
    current_language: str
    enable_large_dataset_warning: bool
    warning_criteria: str
    warning_threshold_rows: int
    warning_threshold_cells: int
    warning_threshold_filesize_mb: int
    bug_report_email: str
    enable_crash_reporting: bool

    user_app_settings_dir: Path
    recents_file: Path
    settings_file: Path
    usr_recents_file: Path
    usr_settings_file: Path
    default_settings_file: Path
    static_dir: Path
    user_logs_dir: Path

    def save(self) -> None: ...
    def render_vars(self, query: str) -> str: ...


@runtime_checkable
class IThemeManager(Protocol):
    """Protocol for theme management."""

    @property
    def current_theme(self) -> Any | None: ...

    def set_theme(self, name: str) -> bool: ...
    def list_themes(self) -> list[str]: ...
    def get_theme(self, name: str) -> Any | None: ...
    def export_theme(self, theme_name: str, export_path: Path) -> bool: ...
    def import_theme(self, import_path: Path) -> str | None: ...
    def delete_theme(self, name: str) -> bool: ...


@runtime_checkable
class ITranslator(Protocol):
    """Protocol for internationalization."""

    def t(self, key: str, **kwargs: Any) -> str: ...
    def set_locale(self, code: str) -> bool: ...
    def get_available_locales(self) -> list[Any]: ...


# ---------------------------------------------------------------------------
# Application service protocols
# ---------------------------------------------------------------------------

@runtime_checkable
class IEventBus(Protocol):
    """Protocol for publish-subscribe event bus."""

    def subscribe(self, event_type: type, handler: Any) -> None: ...
    def unsubscribe(self, event_type: type, handler: Any) -> None: ...
    def publish(self, event: Any) -> None: ...


@runtime_checkable
class IWindowService(Protocol):
    """Protocol for window management."""

    def create_window(self, file_path: Path | None = None) -> Any: ...
    def remove_window(self, window: Any) -> None: ...
    def window_count(self) -> int: ...


# ---------------------------------------------------------------------------
# Plugin protocols
# ---------------------------------------------------------------------------

class IPlugin(ABC):
    """Abstract base class for plugins."""

    name: str = ""
    version: str = "1.0"
    author: str = ""
    description: str = ""

    @abstractmethod
    def activate(self, context: Any) -> None:
        """Called when the plugin is activated."""
        ...

    @abstractmethod
    def deactivate(self) -> None:
        """Called when the plugin is deactivated."""
        ...
